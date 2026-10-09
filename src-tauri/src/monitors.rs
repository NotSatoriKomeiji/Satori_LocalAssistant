//! DDC/CI external monitor adapter; no WMI fallback or simulated dimming overlay.
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Monitor {
    pub id: String,
    pub name: String,
    pub value: u8,
}
#[cfg(not(windows))]
pub fn read() -> Result<Vec<Monitor>, String> {
    Err("外接屏控制需要Windows桌面版".into())
}
#[cfg(not(windows))]
pub fn set(_: &str, _: u8, _: u8) -> Result<(), String> {
    Err("外接屏控制需要Windows桌面版".into())
}
#[cfg(windows)]
mod win {
    use super::*;
    use std::{ffi::c_void, mem::size_of, ptr::null_mut};
    type Handle = *mut c_void;
    #[repr(C)]
    #[derive(Default)]
    struct Rect {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    }
    #[repr(C)]
    struct Info {
        size: u32,
        monitor: Rect,
        work: Rect,
        flags: u32,
        device: [u16; 32],
    }
    #[repr(C)]
    struct Physical {
        handle: Handle,
        description: [u16; 128],
    }
    #[repr(C)]
    struct Display {
        size: u32,
        name: [u16; 32],
        description: [u16; 128],
        flags: u32,
        id: [u16; 128],
        key: [u16; 128],
    }
    #[link(name = "user32")]
    extern "system" {
        fn EnumDisplayMonitors(
            dc: Handle,
            clip: *const Rect,
            callback: unsafe extern "system" fn(Handle, Handle, *mut Rect, isize) -> i32,
            data: isize,
        ) -> i32;
        fn GetMonitorInfoW(h: Handle, info: *mut Info) -> i32;
        fn EnumDisplayDevicesW(
            device: *const u16,
            index: u32,
            info: *mut Display,
            flags: u32,
        ) -> i32;
    }
    #[link(name = "dxva2")]
    extern "system" {
        fn GetNumberOfPhysicalMonitorsFromHMONITOR(h: Handle, count: *mut u32) -> i32;
        fn GetPhysicalMonitorsFromHMONITOR(h: Handle, count: u32, monitors: *mut Physical) -> i32;
        fn DestroyPhysicalMonitor(h: Handle) -> i32;
        fn GetMonitorBrightness(h: Handle, min: *mut u32, current: *mut u32, max: *mut u32) -> i32;
        fn SetMonitorBrightness(h: Handle, value: u32) -> i32;
    }
    struct Device {
        handle: Handle,
        monitor: Monitor,
        min: u32,
        max: u32,
    }
    impl Drop for Device {
        fn drop(&mut self) {
            unsafe {
                DestroyPhysicalMonitor(self.handle);
            }
        }
    }
    fn string(x: &[u16]) -> String {
        String::from_utf16_lossy(&x[..x.iter().position(|v| *v == 0).unwrap_or(x.len())])
    }
    unsafe extern "system" fn collect(h: Handle, _: Handle, _: *mut Rect, data: isize) -> i32 {
        let rows = &mut *(data as *mut Vec<Device>);
        let mut count = 0;
        if GetNumberOfPhysicalMonitorsFromHMONITOR(h, &mut count) == 0 || count == 0 || count > 16 {
            return 1;
        }
        let mut ps: Vec<Physical> = (0..count).map(|_| std::mem::zeroed()).collect();
        if GetPhysicalMonitorsFromHMONITOR(h, count, ps.as_mut_ptr()) == 0 {
            return 1;
        }
        let mut info: Info = std::mem::zeroed();
        info.size = size_of::<Info>() as u32;
        let mut display: Display = std::mem::zeroed();
        display.size = size_of::<Display>() as u32;
        let identified = GetMonitorInfoW(h, &mut info) != 0
            && EnumDisplayDevicesW(info.device.as_ptr(), 0, &mut display, 1) != 0;
        for p in ps {
            let (mut min, mut value, mut max) = (0, 0, 0);
            // Ambiguous mirrored/MST physical mappings are omitted rather than assigned the wrong preference.
            if !identified
                || count != 1
                || GetMonitorBrightness(p.handle, &mut min, &mut value, &mut max) == 0
                || max <= min
                || value < min
                || value > max
            {
                DestroyPhysicalMonitor(p.handle);
                continue;
            }
            let id = string(&display.id);
            if id.is_empty() {
                DestroyPhysicalMonitor(p.handle);
                continue;
            }
            rows.push(Device {
                handle: p.handle,
                min,
                max,
                monitor: Monitor {
                    id,
                    name: string(&p.description),
                    value: (((value - min) as u64 * 100) / (max - min) as u64) as u8,
                },
            });
        }
        1
    }
    fn devices() -> Result<Vec<Device>, String> {
        let mut rows = Vec::<Device>::new();
        if unsafe {
            EnumDisplayMonitors(
                null_mut(),
                std::ptr::null(),
                collect,
                &mut rows as *mut _ as isize,
            )
        } == 0
        {
            return Err("无法读取外接显示器".into());
        }
        Ok(rows)
    }
    pub fn read() -> Result<Vec<Monitor>, String> {
        Ok(devices()?.into_iter().map(|d| d.monitor.clone()).collect())
    }
    pub fn set(id: &str, before: u8, value: u8) -> Result<(), String> {
        if !(10..=100).contains(&value) {
            return Err("亮度超出安全范围".into());
        }
        let devices = devices()?;
        let matches: Vec<_> = devices.iter().filter(|d| d.monitor.id == id).collect();
        if matches.len() != 1 {
            return Err("显示器已断开或标识不唯一".into());
        }
        let d = matches[0];
        if d.monitor.value.abs_diff(before) > 1 {
            return Err("你已手动修改亮度，本次调节取消".into());
        }
        let raw = d.min + ((d.max - d.min) as u64 * u64::from(value) / 100) as u32;
        if unsafe { SetMonitorBrightness(d.handle, raw) } == 0 {
            return Err("显示器未接受亮度调节，请检查屏幕菜单中的DDC/CI".into());
        }
        let (mut min, mut current, mut max) = (0, 0, 0);
        if unsafe { GetMonitorBrightness(d.handle, &mut min, &mut current, &mut max) } == 0
            || max <= min
        {
            return Err("亮度可能已改变，但无法读回；请查看屏幕".into());
        }
        let actual = ((current.saturating_sub(min) as u64 * 100) / (max - min) as u64) as u8;
        if actual.abs_diff(value) > 2 {
            return Err("显示器返回的亮度与请求不符".into());
        }
        Ok(())
    }
}
#[cfg(windows)]
pub use win::{read, set};
