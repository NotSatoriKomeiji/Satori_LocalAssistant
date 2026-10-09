//! Native event subscriptions. Blocking message pump; no desktop/audio polling.
use super::{identity, Platform, Signal, Snapshot, Waker};
use crate::{model::valid_volume, Result};
use std::{cell::RefCell, sync::mpsc, thread::JoinHandle, time::Duration};
use windows::{
    core::{implement, GUID, PCWSTR, PWSTR},
    Win32::{
        Foundation::{CloseHandle, HMODULE, HWND, LPARAM, PROPERTYKEY, WPARAM},
        Media::Audio::{
            eConsole, eRender, EDataFlow, ERole,
            Endpoints::{
                IAudioEndpointVolume, IAudioEndpointVolumeCallback,
                IAudioEndpointVolumeCallback_Impl,
            },
            IMMDeviceEnumerator, IMMNotificationClient, IMMNotificationClient_Impl,
            MMDeviceEnumerator, AUDIO_VOLUME_NOTIFICATION_DATA, DEVICE_STATE,
        },
        System::{
            Com::{
                CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL,
                COINIT_MULTITHREADED,
            },
            SystemInformation::GetLocalTime,
            Threading::{
                GetCurrentProcessId, GetCurrentThreadId, OpenProcess, QueryFullProcessImageNameW,
                PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
            },
        },
        UI::{
            Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK},
            WindowsAndMessaging::{
                DispatchMessageW, GetClassNameW, GetForegroundWindow, GetMessageW,
                GetWindowThreadProcessId, PeekMessageW, PostThreadMessageW, TranslateMessage,
                EVENT_SYSTEM_FOREGROUND, MSG, PM_NOREMOVE, WINEVENT_OUTOFCONTEXT,
                WINEVENT_SKIPOWNPROCESS, WM_APP, WM_QUIT,
            },
        },
    },
};

const OWN_CONTEXT: GUID = GUID::from_u128(0xb6c202fd_fa84_47af_a811_f30c769eccea);
const REBIND: u32 = WM_APP + 41;
thread_local! {static FOREGROUND_WAKER:RefCell<Option<Waker>>=RefCell::new(None);}
fn native_error(error: windows::core::Error) -> crate::Error {
    crate::Error::Rule(format!("Windows 接口失败（{}）", error.code().0))
}
struct Com;
impl Com {
    fn new() -> Result<Self> {
        unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED)
                .ok()
                .map_err(native_error)?;
        }
        Ok(Self)
    }
}
impl Drop for Com {
    fn drop(&mut self) {
        unsafe {
            CoUninitialize();
        }
    }
}
unsafe extern "system" fn foreground(
    _: HWINEVENTHOOK,
    _: u32,
    _: HWND,
    _: i32,
    _: i32,
    _: u32,
    _: u32,
) {
    FOREGROUND_WAKER.with(|w| {
        if let Some(wake) = w.borrow().as_ref() {
            wake(Signal::Foreground);
        }
    });
}
struct Hook(HWINEVENTHOOK);
impl Drop for Hook {
    fn drop(&mut self) {
        unsafe {
            let _ = UnhookWinEvent(self.0);
        }
    }
}

#[implement(IAudioEndpointVolumeCallback)]
struct VolumeNotify {
    wake: Waker,
}
impl IAudioEndpointVolumeCallback_Impl for VolumeNotify_Impl {
    fn OnNotify(&self, data: *mut AUDIO_VOLUME_NOTIFICATION_DATA) -> windows::core::Result<()> {
        // Borrow only during this callback. We never retain the OS-owned pointer.
        if let Some(data) = unsafe { data.as_ref() } {
            if data.guidEventContext != OWN_CONTEXT {
                (self.wake)(Signal::Volume);
            }
        }
        Ok(())
    }
}
#[implement(IMMNotificationClient)]
struct DeviceNotify {
    thread: u32,
}
impl DeviceNotify {
    fn rebind(&self) {
        unsafe {
            let _ = PostThreadMessageW(self.thread, REBIND, WPARAM(0), LPARAM(0));
        }
    }
}
impl IMMNotificationClient_Impl for DeviceNotify_Impl {
    fn OnDefaultDeviceChanged(
        &self,
        flow: EDataFlow,
        role: ERole,
        _: &PCWSTR,
    ) -> windows::core::Result<()> {
        if flow == eRender && role == eConsole {
            self.rebind();
        }
        Ok(())
    }
    fn OnDeviceStateChanged(&self, _: &PCWSTR, _: DEVICE_STATE) -> windows::core::Result<()> {
        self.rebind();
        Ok(())
    }
    fn OnDeviceAdded(&self, _: &PCWSTR) -> windows::core::Result<()> {
        self.rebind();
        Ok(())
    }
    fn OnDeviceRemoved(&self, _: &PCWSTR) -> windows::core::Result<()> {
        self.rebind();
        Ok(())
    }
    fn OnPropertyValueChanged(&self, _: &PCWSTR, _: &PROPERTYKEY) -> windows::core::Result<()> {
        Ok(())
    }
}
struct VolumeBinding {
    endpoint: IAudioEndpointVolume,
    callback: IAudioEndpointVolumeCallback,
}
impl Drop for VolumeBinding {
    fn drop(&mut self) {
        unsafe {
            let _ = self.endpoint.UnregisterControlChangeNotify(&self.callback);
        }
    }
}
fn bind_volume(enumerator: &IMMDeviceEnumerator, wake: Waker) -> Result<VolumeBinding> {
    unsafe {
        let device = enumerator
            .GetDefaultAudioEndpoint(eRender, eConsole)
            .map_err(native_error)?;
        let endpoint = device
            .Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None)
            .map_err(native_error)?;
        let callback: IAudioEndpointVolumeCallback = VolumeNotify { wake }.into();
        endpoint
            .RegisterControlChangeNotify(&callback)
            .map_err(native_error)?;
        Ok(VolumeBinding { endpoint, callback })
    }
}
struct DeviceBinding {
    enumerator: IMMDeviceEnumerator,
    callback: IMMNotificationClient,
}
impl Drop for DeviceBinding {
    fn drop(&mut self) {
        unsafe {
            let _ = self
                .enumerator
                .UnregisterEndpointNotificationCallback(&self.callback);
        }
    }
}
struct EventWatcher {
    thread: u32,
    join: Option<JoinHandle<()>>,
}
impl EventWatcher {
    fn start(wake: Waker) -> Result<Self> {
        let (ready, receiver) = mpsc::channel();
        let join = std::thread::Builder::new()
            .name("habitos-native-events".into())
            .spawn(move || {
                let result = (|| -> Result<()> {
                    let _com = Com::new()?;
                    let thread = unsafe { GetCurrentThreadId() };
                    let mut message = MSG::default();
                    // Create the message queue before publishing the thread ID.
                    unsafe {
                        let _ = PeekMessageW(&mut message, None, 0, 0, PM_NOREMOVE);
                    }
                    FOREGROUND_WAKER.with(|w| *w.borrow_mut() = Some(wake.clone()));
                    let hook = unsafe {
                        SetWinEventHook(
                            EVENT_SYSTEM_FOREGROUND,
                            EVENT_SYSTEM_FOREGROUND,
                            None::<HMODULE>,
                            Some(foreground),
                            0,
                            0,
                            WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
                        )
                    };
                    if hook.0.is_null() {
                        return Err(crate::rule("无法注册前台窗口事件"));
                    }
                    let _hook = Hook(hook);
                    let enumerator: IMMDeviceEnumerator = unsafe {
                        CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
                            .map_err(native_error)?
                    };
                    let callback: IMMNotificationClient = DeviceNotify { thread }.into();
                    unsafe {
                        enumerator
                            .RegisterEndpointNotificationCallback(&callback)
                            .map_err(native_error)?;
                    }
                    let _devices = DeviceBinding {
                        enumerator: enumerator.clone(),
                        callback,
                    };
                    // No audio endpoint is legitimate. Device events can attach it later.
                    let mut audio = bind_volume(&enumerator, wake.clone()).ok();
                    let _ = ready.send(Ok(thread));
                    loop {
                        let received = unsafe { GetMessageW(&mut message, None, 0, 0) }.0;
                        if received <= 0 {
                            break;
                        }
                        if message.message == REBIND {
                            drop(audio.take());
                            audio = bind_volume(&enumerator, wake.clone()).ok();
                            wake(Signal::Device);
                        } else {
                            unsafe {
                                let _ = TranslateMessage(&message);
                                DispatchMessageW(&message);
                            }
                        }
                    }
                    drop(audio);
                    FOREGROUND_WAKER.with(|w| *w.borrow_mut() = None);
                    Ok(())
                })();
                if let Err(error) = result {
                    let _ = ready.send(Err(error));
                }
            })
            .map_err(|_| crate::rule("无法启动原生事件线程"))?;
        match receiver
            .recv()
            .map_err(|_| crate::rule("原生事件线程未就绪"))?
        {
            Ok(thread) => Ok(Self {
                thread,
                join: Some(join),
            }),
            Err(error) => {
                let _ = join.join();
                Err(error)
            }
        }
    }
}
impl Drop for EventWatcher {
    fn drop(&mut self) {
        unsafe {
            let _ = PostThreadMessageW(self.thread, WM_QUIT, WPARAM(0), LPARAM(0));
        }
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

pub struct WindowsPlatform {
    watcher: Option<EventWatcher>,
    wake: Option<Waker>,
    last_external: Option<(String, String, String)>,
    _com: Com,
}
impl WindowsPlatform {
    pub fn new() -> Result<Self> {
        Ok(Self {
            watcher: None,
            wake: None,
            last_external: None,
            _com: Com::new()?,
        })
    }
    fn app(&mut self) -> Result<(String, String, String)> {
        unsafe {
            let hwnd = GetForegroundWindow();
            if hwnd.0.is_null() {
                return Err(crate::rule("当前没有可识别的前台窗口"));
            }
            let mut pid = 0;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            if pid == GetCurrentProcessId() {
                return self
                    .last_external
                    .clone()
                    .ok_or_else(|| crate::rule("等待外部软件或桌面事件"));
            }
            let handle =
                OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).map_err(native_error)?;
            let mut buffer = vec![0u16; 32768];
            let mut size = buffer.len() as u32;
            let result = QueryFullProcessImageNameW(
                handle,
                PROCESS_NAME_WIN32,
                PWSTR(buffer.as_mut_ptr()),
                &mut size,
            );
            let _ = CloseHandle(handle);
            result.map_err(native_error)?;
            let path = String::from_utf16_lossy(&buffer[..size as usize]);
            let name = path
                .rsplit(['\\', '/'])
                .next()
                .unwrap_or("unknown.exe")
                .to_string();
            let mut class = [0u16; 128];
            let len = GetClassNameW(hwnd, &mut class).max(0) as usize;
            let class = String::from_utf16_lossy(&class[..len]);
            let activity = if ["Progman", "WorkerW", "Shell_TrayWnd"].contains(&class.as_str()) {
                "desktop"
            } else {
                "application"
            };
            let app = (identity(&path), name, activity.into());
            self.last_external = Some(app.clone());
            Ok(app)
        }
    }
    fn endpoint(&self) -> Result<(String, IAudioEndpointVolume)> {
        unsafe {
            let enumerator: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).map_err(native_error)?;
            let device = enumerator
                .GetDefaultAudioEndpoint(eRender, eConsole)
                .map_err(native_error)?;
            let pointer = device.GetId().map_err(native_error)?;
            let id = pointer.to_string();
            CoTaskMemFree(Some(pointer.0.cast()));
            let id = id.map_err(|_| crate::rule("音频设备标识格式无效"))?;
            let endpoint = device
                .Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None)
                .map_err(native_error)?;
            Ok((identity(&id), endpoint))
        }
    }
}
impl Platform for WindowsPlatform {
    fn open_website(&mut self, input: &str) -> Result<()> {
        use windows::{
            core::w,
            Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL},
        };
        let origin = crate::website::origin(input)?;
        let text: Vec<u16> = origin.encode_utf16().chain(Some(0)).collect();
        let result = unsafe {
            ShellExecuteW(
                None,
                w!("open"),
                PCWSTR(text.as_ptr()),
                None,
                None,
                SW_SHOWNORMAL,
            )
        };
        if result.0 as isize <= 32 {
            return Err(crate::rule("无法由默认浏览器打开站点"));
        }
        Ok(())
    }

    fn startup_status(&mut self) -> Result<crate::model::StartupStatus> {
        let path = std::env::current_exe().map_err(|_| crate::rule("无法识别当前程序位置"))?;
        let expected = path.to_str().and_then(|p| crate::startup::command(p).ok());
        let value = startup_value()?;
        Ok(crate::model::StartupStatus {
            supported: true,
            enabled: expected.as_ref().map_or_else(
                || value.is_some(),
                |e| {
                    value
                        .as_ref()
                        .is_some_and(|s| s.to_lowercase() == e.to_lowercase())
                },
            ),
            simulated: false,
            message: if expected.is_none() {
                "当前路径过长或无效，请移到较短目录后开启；仍可关闭旧启动项".into()
            } else if value.is_some()
                && value.as_ref().is_none_or(|s| {
                    expected
                        .as_ref()
                        .is_none_or(|e| s.to_lowercase() != e.to_lowercase())
                })
            {
                "启动项指向其他位置，重新开启会更新路径".into()
            } else {
                "登录当前Windows用户后在后台启动".into()
            },
        })
    }
    fn set_startup(&mut self, enabled: bool) -> Result<()> {
        set_startup_value(enabled)
    }

    fn subscribe(&mut self, wake: Waker) -> Result<()> {
        self.wake = Some(wake.clone());
        self.watcher = Some(EventWatcher::start(wake)?);
        Ok(())
    }
    fn set_paused(&mut self, paused: bool) -> Result<()> {
        if paused {
            self.watcher.take();
        } else if self.watcher.is_none() {
            if let Some(wake) = &self.wake {
                self.watcher = Some(EventWatcher::start(wake.clone())?);
            }
        }
        Ok(())
    }
    fn next_clock_in(&self) -> Duration {
        let t = unsafe { GetLocalTime() };
        Duration::from_millis(
            3_600_000
                - (u64::from(t.wMinute) * 60_000
                    + u64::from(t.wSecond) * 1000
                    + u64::from(t.wMilliseconds)),
        )
    }
    fn snapshot(&mut self) -> Result<Snapshot> {
        let (app_id, app_name, activity) = self.app()?;
        // Foreground events remain useful when no playback endpoint is available.
        let audio = self.endpoint().and_then(|(device, endpoint)| {
            let volume = unsafe {
                endpoint
                    .GetMasterVolumeLevelScalar()
                    .map_err(native_error)?
            };
            Ok((device, volume as f64))
        });
        let audio_available = audio.is_ok();
        let (device, volume) = audio.unwrap_or_else(|_| (identity("no-default-audio"), 0.0));
        let time = unsafe { GetLocalTime() };
        let hour = time.wHour as u8;
        let weekday = ((time.wDayOfWeek + 6) % 7) as u8;
        let local_day =
            i64::from(time.wYear) * 10000 + i64::from(time.wMonth) * 100 + i64::from(time.wDay);
        Ok(Snapshot {
            app_id,
            app_name,
            device,
            volume,
            audio_available,
            hour,
            weekday,
            local_day,
            activity,
        })
    }
    fn launch_program(&mut self, path: &str) -> Result<()> {
        use windows::Win32::System::Threading::{
            CreateProcessW, PROCESS_CREATION_FLAGS, PROCESS_INFORMATION, STARTUPINFOW,
        };
        let file = std::path::Path::new(path);
        if !file.is_absolute()
            || !file.is_file()
            || !file
                .extension()
                .is_some_and(|x| x.eq_ignore_ascii_case("exe"))
        {
            return Err(crate::rule("启动目标已移动或不再是 exe"));
        }
        let executable: Vec<u16> = path.encode_utf16().chain(Some(0)).collect();
        let directory: Vec<u16> = file
            .parent()
            .ok_or_else(|| crate::rule("启动目录无效"))?
            .to_string_lossy()
            .encode_utf16()
            .chain(Some(0))
            .collect();
        let mut command: Vec<u16> = format!("\"{path}\"")
            .encode_utf16()
            .chain(Some(0))
            .collect();
        let info = STARTUPINFOW {
            cb: std::mem::size_of::<STARTUPINFOW>() as u32,
            ..Default::default()
        };
        let mut process = PROCESS_INFORMATION::default();
        unsafe {
            CreateProcessW(
                PCWSTR(executable.as_ptr()),
                Some(PWSTR(command.as_mut_ptr())),
                None,
                None,
                false,
                PROCESS_CREATION_FLAGS(0),
                None,
                PCWSTR(directory.as_ptr()),
                &info,
                &mut process,
            )
            .map_err(native_error)?;
            let _ = CloseHandle(process.hThread);
            let _ = CloseHandle(process.hProcess);
        }
        Ok(())
    }
    fn set_volume(&mut self, expected: &Snapshot, value: f64) -> Result<()> {
        if !expected.audio_available || !valid_volume(value) {
            return Err(crate::rule("音量范围无效"));
        }
        let (id, _, _) = self.app()?;
        let (device, endpoint) = self.endpoint()?;
        let current = unsafe {
            endpoint
                .GetMasterVolumeLevelScalar()
                .map_err(native_error)?
        } as f64;
        if id != expected.app_id
            || device != expected.device
            || (current - expected.volume).abs() > 0.02
        {
            return Err(crate::rule("执行前状态发生变化，动作取消"));
        }
        unsafe {
            endpoint
                .SetMasterVolumeLevelScalar(value as f32, &OWN_CONTEXT)
                .map_err(native_error)?;
        }
        Ok(())
    }
}
impl Drop for WindowsPlatform {
    fn drop(&mut self) {
        self.watcher.take();
    }
}

pub fn choose_program(owner: isize) -> Result<Option<String>> {
    use windows::Win32::UI::Controls::Dialogs::{
        CommDlgExtendedError, GetOpenFileNameW, OFN_EXPLORER, OFN_FILEMUSTEXIST, OFN_NOCHANGEDIR,
        OFN_PATHMUSTEXIST, OPENFILENAMEW,
    };
    let mut buffer = vec![0u16; 32768];
    let filter: Vec<u16> = "Windows 应用 (*.exe)\0*.exe\0\0".encode_utf16().collect();
    let title: Vec<u16> = "选择可推荐应用（点击卡片才会启动）\0"
        .encode_utf16()
        .collect();
    let mut dialog = OPENFILENAMEW {
        lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32,
        hwndOwner: HWND(owner as *mut _),
        lpstrFilter: PCWSTR(filter.as_ptr()),
        lpstrFile: PWSTR(buffer.as_mut_ptr()),
        nMaxFile: buffer.len() as u32,
        lpstrTitle: PCWSTR(title.as_ptr()),
        Flags: OFN_EXPLORER | OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR,
        ..Default::default()
    };
    if unsafe { GetOpenFileNameW(&mut dialog) }.as_bool() {
        let end = buffer.iter().position(|x| *x == 0).unwrap_or(buffer.len());
        Ok(Some(String::from_utf16_lossy(&buffer[..end])))
    } else if unsafe { CommDlgExtendedError() }.0 == 0 {
        Ok(None)
    } else {
        Err(crate::rule("无法打开应用选择窗口"))
    }
}
pub fn show_card_inactive(handle: isize) -> Result<()> {
    use windows::Win32::UI::WindowsAndMessaging::{
        SetWindowPos, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW,
    };
    unsafe {
        SetWindowPos(
            HWND(handle as *mut _),
            Some(HWND_TOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW,
        )
        .map_err(native_error)
    }
}

fn startup_value() -> Result<Option<String>> {
    use windows::Win32::{
        Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS},
        System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_SZ},
    };
    let key: Vec<u16> = "Software\\Microsoft\\Windows\\CurrentVersion\\Run\0"
        .encode_utf16()
        .collect();
    let name: Vec<u16> = "Satori\0".encode_utf16().collect();
    let mut buffer = [0u16; 1024];
    let mut size = (buffer.len() * 2) as u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            PCWSTR(key.as_ptr()),
            PCWSTR(name.as_ptr()),
            RRF_RT_REG_SZ,
            None,
            Some(buffer.as_mut_ptr().cast()),
            Some(&mut size),
        )
    };
    if status == ERROR_FILE_NOT_FOUND {
        return Ok(None);
    }
    if status != ERROR_SUCCESS {
        return Err(crate::rule("无法读取当前用户启动项"));
    }
    let end = buffer.iter().position(|c| *c == 0).unwrap_or(buffer.len());
    Ok(Some(String::from_utf16_lossy(&buffer[..end])))
}
fn set_startup_value(enabled: bool) -> Result<()> {
    use windows::Win32::{
        Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS},
        System::Registry::{
            RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegSetValueExW, HKEY, HKEY_CURRENT_USER,
            KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SZ,
        },
    };
    let value = if enabled {
        let executable =
            std::env::current_exe().map_err(|_| crate::rule("无法识别当前程序位置"))?;
        crate::startup::command(
            executable
                .to_str()
                .ok_or_else(|| crate::rule("程序路径无效"))?,
        )?
    } else {
        String::new()
    };
    let key: Vec<u16> = "Software\\Microsoft\\Windows\\CurrentVersion\\Run\0"
        .encode_utf16()
        .collect();
    let name: Vec<u16> = "Satori\0".encode_utf16().collect();
    let mut handle = HKEY::default();
    let status = unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(key.as_ptr()),
            None,
            PCWSTR::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            None,
            &mut handle,
            None,
        )
    };
    if status != ERROR_SUCCESS {
        return Err(crate::rule("无法修改当前用户启动项"));
    }
    let status = if enabled {
        let bytes: Vec<u8> = value
            .encode_utf16()
            .chain(std::iter::once(0))
            .flat_map(u16::to_le_bytes)
            .collect();
        unsafe { RegSetValueExW(handle, PCWSTR(name.as_ptr()), None, REG_SZ, Some(&bytes)) }
    } else {
        unsafe { RegDeleteValueW(handle, PCWSTR(name.as_ptr())) }
    };
    unsafe {
        let _ = RegCloseKey(handle);
    }
    if status != ERROR_SUCCESS && (status != ERROR_FILE_NOT_FOUND || enabled) {
        return Err(crate::rule("无法修改当前用户启动项"));
    }
    Ok(())
}
