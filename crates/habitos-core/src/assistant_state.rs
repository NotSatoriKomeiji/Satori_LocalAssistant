//! Commit auxiliary settings to disk before exposing a changed in-memory state.
use crate::{adjustment::Preferences, model::now, quick_words::QuickWords};
use serde::{Deserialize, Serialize};
use std::{fs, io::Write, path::Path};

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Saved {
    pub quick_words: QuickWords,
    pub brightness: Preferences,
}

impl Saved {
    pub fn load(path: &Path) -> Result<Self, String> {
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(_) => return Err("无法读取助手设置".into()),
        };
        let old: serde_json::Value =
            serde_json::from_slice(&bytes).map_err(|_| "助手设置损坏，请保留文件")?;
        let mut saved = Self::default();
        if old.get("quick_words").is_some() {
            saved = serde_json::from_value(old).map_err(|_| "助手设置无效")?;
        } else if let Some(words) = old.get("phrases").and_then(serde_json::Value::as_array) {
            saved.quick_words.words = words
                .iter()
                .filter_map(|w| w.get("text").and_then(serde_json::Value::as_str))
                .map(|text| crate::quick_words::Word {
                    text: text.into(),
                    uses: 2,
                    last: now(),
                    pinned: true,
                })
                .collect();
        }
        saved.quick_words.limit = saved.quick_words.limit.clamp(1, 200);
        Ok(saved)
    }

    /// Return `false` from the edit for a no-op. Failed commits leave `self` intact.
    pub fn update(
        &mut self,
        path: &Path,
        edit: impl FnOnce(&mut Self) -> Result<bool, String>,
    ) -> Result<bool, String> {
        let mut candidate = self.clone();
        if !edit(&mut candidate)? {
            return Ok(false);
        }
        let bytes = serde_json::to_vec_pretty(&candidate).map_err(|_| "保存失败")?;
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        let temporary = parent.join(format!(".satori-{}.tmp", uuid::Uuid::new_v4()));
        struct Cleanup(std::path::PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = fs::remove_file(&self.0);
            }
        }
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let _cleanup;
        let mut file = options.open(&temporary).map_err(|_| "无法写入助手设置")?;
        _cleanup = Cleanup(temporary.clone());
        file.write_all(&bytes).map_err(|_| "无法写入助手设置")?;
        file.sync_all().map_err(|_| "无法同步助手设置")?;
        drop(file);
        fs::rename(&temporary, path).map_err(|_| "无法保存助手设置")?;
        *self = candidate;
        Ok(true)
    }
}
