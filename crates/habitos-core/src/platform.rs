use crate::{model::valid_volume, Result};

#[derive(Debug, Clone)]
pub struct Snapshot {
    pub app_id: String,
    pub app_name: String,
    pub device: String,
    pub hour: u8,
    pub weekday: u8,
    pub local_day: i64,
    pub volume: f64,
    /// False means no audio reading; volume is ignored, never learned or executed.
    pub audio_available: bool,
    pub activity: String,
}
#[derive(Debug, Clone, Copy)]
pub enum Signal {
    Foreground = 1,
    Volume = 2,
    Device = 4,
}
pub type Waker = std::sync::Arc<dyn Fn(Signal) + Send + Sync>;
pub trait Platform {
    fn snapshot(&mut self) -> Result<Snapshot>;
    /// Must recheck app, endpoint and old value immediately before writing.
    fn set_volume(&mut self, expected: &Snapshot, value: f64) -> Result<()>;
    fn launch_program(&mut self, _: &str) -> Result<()> {
        Err(crate::rule("此平台不支持启动软件"))
    }
    fn open_website(&mut self, _: &str) -> Result<()> {
        Err(crate::rule("此平台不支持打开网站"))
    }
    fn startup_status(&mut self) -> Result<crate::model::StartupStatus> {
        Ok(crate::model::StartupStatus::default())
    }
    fn set_startup(&mut self, _: bool) -> Result<()> {
        Err(crate::rule("此平台不支持开机自启动"))
    }
    fn subscribe(&mut self, _: Waker) -> Result<()> {
        Ok(())
    }
    fn set_paused(&mut self, _: bool) -> Result<()> {
        Ok(())
    }
    fn next_clock_in(&self) -> std::time::Duration {
        std::time::Duration::from_secs((3600 - (crate::model::now() % 3600)) as u64)
    }
    fn demo_scene(&mut self, _: &str) -> Result<()> {
        Err(crate::rule("演示操作只能在演示模式使用"))
    }
    fn demo_volume(&mut self, _: f64) -> Result<()> {
        Err(crate::rule("演示操作只能在演示模式使用"))
    }
}

pub struct DemoPlatform {
    pub current: Snapshot,
    startup_enabled: bool,
}
impl Default for DemoPlatform {
    fn default() -> Self {
        Self {
            startup_enabled: false,
            current: Snapshot {
                app_id: identity("demo:game.exe"),
                app_name: "Game.exe（模拟）".into(),
                device: identity("demo:speakers"),
                hour: 20,
                weekday: 3,
                local_day: 20261008,
                volume: 0.50,
                audio_available: true,
                activity: "application".into(),
            },
        }
    }
}
impl Platform for DemoPlatform {
    fn open_website(&mut self, url: &str) -> Result<()> {
        crate::website::origin(url)?;
        Ok(())
    }
    fn startup_status(&mut self) -> Result<crate::model::StartupStatus> {
        Ok(crate::model::StartupStatus {
            supported: true,
            enabled: self.startup_enabled,
            simulated: true,
            message: "演示状态，不修改系统启动项".into(),
        })
    }
    fn set_startup(&mut self, enabled: bool) -> Result<()> {
        self.startup_enabled = enabled;
        Ok(())
    }
    fn snapshot(&mut self) -> Result<Snapshot> {
        Ok(self.current.clone())
    }
    fn set_volume(&mut self, expected: &Snapshot, value: f64) -> Result<()> {
        if !valid_volume(value)
            || expected.app_id != self.current.app_id
            || expected.device != self.current.device
            || (expected.volume - self.current.volume).abs() > 0.02
        {
            return Err(crate::rule("模拟场景已变化"));
        }
        self.current.volume = value;
        Ok(())
    }
    fn launch_program(&mut self, _: &str) -> Result<()> {
        Ok(())
    }
    fn demo_scene(&mut self, scene: &str) -> Result<()> {
        let (path, name) = match scene {
            "game" => ("demo:game.exe", "Game.exe（模拟）"),
            "editor" => ("demo:editor.exe", "Editor.exe（模拟）"),
            "browser" => ("demo:browser.exe", "Browser.exe（模拟）"),
            "desktop" => ("demo:desktop", "桌面（模拟）"),
            "sensitive" => ("demo:keepass.exe", "KeePass.exe"),
            _ => return Err(crate::rule("未知演示场景")),
        };
        self.current.app_id = identity(path);
        self.current.app_name = name.into();
        self.current.activity = if scene == "desktop" {
            "desktop"
        } else {
            "application"
        }
        .into();
        Ok(())
    }
    fn demo_volume(&mut self, value: f64) -> Result<()> {
        if !valid_volume(value) {
            return Err(crate::rule("音量值无效"));
        }
        self.current.volume = value;
        Ok(())
    }
}
pub fn identity(value: &str) -> String {
    blake3::hash(value.to_lowercase().as_bytes())
        .to_hex()
        .to_string()
}

#[cfg(windows)]
#[path = "windows.rs"]
pub mod windows;
