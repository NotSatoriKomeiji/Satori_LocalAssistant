//! Bounded native-messaging inbox. No network listener and no browsing-history scan.
use crate::{
    model::now,
    rule,
    website::{origin, Visit},
    Result,
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
#[derive(Serialize, Deserialize)]
pub struct Policy {
    pub enabled: bool,
    #[serde(default)]
    pub quick_words_allowed: bool,
    pub pid: u32,
    #[serde(default)]
    pub blocked_domains: Vec<String>,
}
pub fn capture_enabled(state: &crate::model::Status) -> bool {
    state.settings.website_observation
        && !state.settings.paused
        && !state.demo
        && state.current.as_ref().is_some_and(|c| {
            state.apps.iter().any(|p| {
                p.id == c.app_id
                    && p.observe
                    && !p.sensitive
                    && (p.explicit || state.settings.automatic_learning)
            })
        })
}
pub fn write_policy(directory: &Path, enabled: bool) -> Result<()> {
    let data = serde_json::to_vec(&Policy {
        enabled,
        quick_words_allowed: false,
        pid: std::process::id(),
        blocked_domains: Vec::new(),
    })?;
    let path = directory.join("browser-policy.json");
    std::fs::write(path, data).map_err(|_| rule("无法更新网站观察许可"))
}
pub fn write_status_policy(directory: &Path, state: &crate::model::Status) -> Result<()> {
    let policy = Policy {
        enabled: capture_enabled(state),
        quick_words_allowed: !state.demo
            && !state.settings.paused
            && state.settings.automatic_learning
            && state.current.as_ref().is_some_and(|c| {
                state
                    .apps
                    .iter()
                    .any(|p| p.id == c.app_id && p.observe && !p.sensitive)
            }),
        pid: std::process::id(),
        blocked_domains: state
            .apps
            .iter()
            .filter(|p| p.id.starts_with("web_") && (p.sensitive || !p.observe))
            .map(|p| p.name.clone())
            .collect(),
    };
    std::fs::write(
        directory.join("browser-policy.json"),
        serde_json::to_vec(&policy)?,
    )
    .map_err(|_| rule("无法更新网站观察许可"))
}
pub fn enqueue(directory: &Path, visit: &Visit) -> Result<()> {
    let origin = origin(&visit.origin)?;
    let at = now();
    if visit.at > at + 5 || at - visit.at > 30 {
        return Err(rule("访问事件已过期"));
    }
    let inbox = directory.join("browser-inbox");
    std::fs::create_dir_all(&inbox).map_err(|_| rule("无法创建浏览器事件目录"))?;
    if std::fs::read_dir(&inbox)
        .map_err(|_| rule("无法读取事件目录"))?
        .take(64)
        .count()
        >= 64
    {
        return Err(rule("浏览器事件队列已满"));
    }
    let id = uuid::Uuid::new_v4().to_string();
    let tmp = inbox.join(format!("{id}.tmp"));
    let dest = inbox.join(format!("{id}.json"));
    std::fs::write(
        &tmp,
        serde_json::to_vec(&Visit {
            origin,
            at: visit.at,
        })?,
    )
    .map_err(|_| rule("无法写入访问事件"))?;
    std::fs::rename(&tmp, dest).map_err(|_| rule("无法提交访问事件"))?;
    Ok(())
}
pub fn drain(directory: &Path, mut receive: impl FnMut(Visit)) {
    let Ok(entries) = std::fs::read_dir(directory.join("browser-inbox")) else {
        return;
    };
    for path in entries.filter_map(|r| r.ok().map(|e| e.path())).take(64) {
        if path.extension().is_none_or(|e| e != "json") {
            continue;
        }
        if let Ok(meta) = std::fs::metadata(&path) {
            if meta.len() <= 4096 {
                if let Ok(bytes) = std::fs::read(&path) {
                    if let Ok(mut visit) = serde_json::from_slice::<Visit>(&bytes) {
                        if let Ok(clean) = origin(&visit.origin) {
                            visit.origin = clean;
                            receive(visit);
                        }
                    }
                }
            }
        }
        let _ = std::fs::remove_file(path);
    }
}
pub fn data_directory() -> Result<PathBuf> {
    std::env::var_os("APPDATA")
        .map(|p| PathBuf::from(p).join("io.habitos.desktop"))
        .ok_or_else(|| rule("无法定位当前用户数据目录"))
}
#[cfg(windows)]
pub fn native_host() -> Result<()> {
    let expected = format!(
        "chrome-extension://{}/",
        include_str!("../../../browser-extension/extension-id.txt").trim()
    );
    let caller = std::env::args().nth(1).is_some_and(|v| v == expected);
    use windows::{
        core::PWSTR,
        Win32::{
            Foundation::CloseHandle,
            System::Threading::{
                GetExitCodeProcess, OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
                PROCESS_QUERY_LIMITED_INFORMATION,
            },
        },
    };
    let directory = data_directory()?;
    let policy: Option<Policy> = std::fs::read(directory.join("browser-policy.json"))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok());
    let running = caller
        && policy.as_ref().is_some_and(|p| unsafe {
            let Ok(handle) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, p.pid) else {
                return false;
            };
            let mut exit = 0;
            let mut buffer = vec![0u16; 32768];
            let mut len = buffer.len() as u32;
            let alive = GetExitCodeProcess(handle, &mut exit).is_ok()
                && exit == 259
                && QueryFullProcessImageNameW(
                    handle,
                    PROCESS_NAME_WIN32,
                    PWSTR(buffer.as_mut_ptr()),
                    &mut len,
                )
                .is_ok()
                && std::env::current_exe().is_ok_and(|path| {
                    path.to_string_lossy().to_lowercase()
                        == String::from_utf16_lossy(&buffer[..len as usize]).to_lowercase()
                });
            let _ = CloseHandle(handle);
            alive
        });
    use std::io::{Read, Write};
    let mut input = std::io::stdin().lock();
    let mut size = [0u8; 4];
    input.read_exact(&mut size).map_err(|_| rule("消息缺失"))?;
    let n = u32::from_le_bytes(size) as usize;
    if n > 4096 {
        return Err(rule("消息过大"));
    }
    let mut bytes = vec![0; n];
    input
        .read_exact(&mut bytes)
        .map_err(|_| rule("消息不完整"))?;
    let message: serde_json::Value = serde_json::from_slice(&bytes)?;
    let response = if message["type"].as_str() == Some("quick_words") {
        let live: serde_json::Value = std::fs::read(directory.join("quick-words-live.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        let saved: serde_json::Value = std::fs::read(directory.join("assist.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        let enabled = running
            && policy.as_ref().is_some_and(|p| p.quick_words_allowed)
            && saved["quick_words"]["enabled"].as_bool() == Some(true)
            && live["enabled"].as_bool() == Some(true)
            && live["at"]
                .as_i64()
                .is_some_and(|at| (0..=3).contains(&(now() - at)))
            && message["origin"].as_str().is_some_and(|o| {
                origin(o).is_ok()
                    && policy.as_ref().is_some_and(|p| {
                        !p.blocked_domains
                            .iter()
                            .any(|host| o == format!("https://{host}/"))
                    })
            });
        if enabled {
            if let Some(text) = message["text"].as_str() {
                if crate::quick_words::safe_text(text) {
                    let inbox = directory.join("quick-words-inbox");
                    let _ = std::fs::create_dir_all(&inbox);
                    if std::fs::read_dir(&inbox)
                        .map(|e| e.take(64).count() < 64)
                        .unwrap_or(false)
                    {
                        let id = uuid::Uuid::new_v4();
                        let tmp = inbox.join(format!("{id}.tmp"));
                        if std::fs::write(
                            &tmp,
                            serde_json::json!({"text":text,"at":now()}).to_string(),
                        )
                        .is_ok()
                        {
                            let _ = std::fs::rename(tmp, inbox.join(format!("{id}.json")));
                        }
                    }
                }
            }
        }
        serde_json::json!({"enabled":enabled,"words":if enabled{live["words"].clone()}else{serde_json::json!([])}})
    } else {
        let accepted = running
            && policy.as_ref().is_some_and(|p| p.enabled)
            && serde_json::from_value::<Visit>(message).is_ok_and(|v| {
                policy.as_ref().is_some_and(|p| {
                    !p.blocked_domains
                        .iter()
                        .any(|host| v.origin == format!("https://{host}/"))
                }) && enqueue(&directory, &v).is_ok()
            });
        serde_json::json!({"accepted":accepted})
    };
    let output = serde_json::to_vec(&response)?;
    let mut stdout = std::io::stdout().lock();
    stdout
        .write_all(&(output.len() as u32).to_le_bytes())
        .and_then(|_| stdout.write_all(&output))
        .map_err(|_| rule("浏览器响应失败"))
}
#[cfg(windows)]
pub struct Watcher {
    stop: usize,
    thread: Option<std::thread::JoinHandle<()>>,
    directory: PathBuf,
}
#[cfg(windows)]
impl Watcher {
    pub fn start(
        directory: PathBuf,
        receive: std::sync::Arc<dyn Fn(Visit) + Send + Sync>,
    ) -> Result<Self> {
        use windows::{
            core::PCWSTR,
            Win32::{
                Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0},
                Storage::FileSystem::{
                    FindCloseChangeNotification, FindFirstChangeNotificationW,
                    FindNextChangeNotification, FILE_NOTIFY_CHANGE_FILE_NAME,
                },
                System::Threading::{CreateEventW, WaitForMultipleObjects, INFINITE},
            },
        };
        std::fs::create_dir_all(directory.join("browser-inbox"))
            .map_err(|_| rule("无法创建浏览器事件目录"))?;
        let stop = unsafe { CreateEventW(None, true, false, None) }
            .map_err(|_| rule("无法启动浏览器事件等待"))?
            .0 as usize;
        let copy = directory.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        let thread = std::thread::spawn(move || {
            let path: Vec<u16> = copy
                .join("browser-inbox")
                .as_os_str()
                .to_string_lossy()
                .encode_utf16()
                .chain(Some(0))
                .collect();
            let change = unsafe {
                FindFirstChangeNotificationW(
                    PCWSTR(path.as_ptr()),
                    false,
                    FILE_NOTIFY_CHANGE_FILE_NAME,
                )
            };
            let Ok(change) = change else {
                let _ = tx.send(false);
                return;
            };
            let _ = tx.send(true);
            drain(&copy, |v| receive(v));
            loop {
                let event = unsafe {
                    WaitForMultipleObjects(&[HANDLE(stop as *mut _), change], false, INFINITE)
                };
                if event.0 != WAIT_OBJECT_0.0 + 1 {
                    break;
                }
                // Rearm before draining to avoid losing events during consumption.
                if unsafe { FindNextChangeNotification(change) }.is_err() {
                    break;
                }
                drain(&copy, |v| receive(v));
            }
            let _ = unsafe { FindCloseChangeNotification(change) };
        });
        if rx.recv() != Ok(true) {
            let _ = thread.join();
            let _ = unsafe { CloseHandle(HANDLE(stop as *mut _)) };
            return Err(rule("浏览器事件等待启动失败"));
        }
        Ok(Self {
            stop,
            thread: Some(thread),
            directory,
        })
    }
}
#[cfg(windows)]
impl Drop for Watcher {
    fn drop(&mut self) {
        use windows::Win32::{
            Foundation::{CloseHandle, HANDLE},
            System::Threading::SetEvent,
        };
        let _ = write_policy(&self.directory, false);
        let _ = unsafe { SetEvent(HANDLE(self.stop as *mut _)) };
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        let _ = unsafe { CloseHandle(HANDLE(self.stop as *mut _)) };
    }
}
