//! Optional, session-only credentials. No provider request exists on the local path.
use habitos_core::{
    experience::{Advice, Interpretation, Kind},
    model::Status,
    runtime::{Command, Runtime},
};
use serde::Serialize;
use serde_json::json;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Mutex,
};

#[derive(Clone)]
struct Config {
    endpoint: String,
    model: String,
    key: String,
}
#[derive(Serialize)]
pub struct AiStatus {
    pub enabled: bool,
    pub awake: bool,
    pub message: String,
}
pub struct ColdAi {
    config: Mutex<Option<Config>>,
    epoch: AtomicU64,
    awake: AtomicBool,
    message: Mutex<String>,
}
impl Default for ColdAi {
    fn default() -> Self {
        Self {
            config: Mutex::new(None),
            epoch: AtomicU64::new(0),
            awake: AtomicBool::new(false),
            message: Mutex::new("本地模式 · AI 未连接".into()),
        }
    }
}
fn allowed(st: &Status) -> bool {
    !st.settings.paused
        && st.settings.automatic_learning
        && st.current.as_ref().is_some_and(|c| {
            st.apps
                .iter()
                .any(|a| a.id == c.app_id && a.observe && !a.sensitive)
        })
}
pub fn validate(endpoint: &str, model: &str, key: &str) -> Result<(), String> {
    if endpoint.len() > 2048
        || model.trim().is_empty()
        || model.len() > 128
        || key.trim().is_empty()
        || key.len() > 4096
    {
        return Err("请填写接口、模型和密钥".into());
    }
    habitos_core::website::origin(endpoint).map_err(|_| "API 地址须为公开 HTTPS 地址")?;
    if endpoint.contains(['?', '#']) {
        return Err("API 地址不能带查询参数或片段".into());
    }
    Ok(())
}
impl ColdAi {
    pub fn ask(&self, rt: &Runtime, prompt: &str) -> Result<String, String> {
        self.ask_config(rt, prompt, None)
    }
    pub fn ask_at(
        &self,
        rt: &Runtime,
        prompt: &str,
        endpoint: String,
        model: String,
        key: String,
    ) -> Result<String, String> {
        validate(&endpoint, &model, &key)?;
        self.ask_config(
            rt,
            prompt,
            Some(Config {
                endpoint,
                model,
                key,
            }),
        )
    }
    fn ask_config(
        &self,
        rt: &Runtime,
        prompt: &str,
        explicit: Option<Config>,
    ) -> Result<String, String> {
        if prompt.trim().is_empty() || prompt.len() > 8192 {
            return Err("问题为空或过长".into());
        }
        let config_guard = self.config.lock().map_err(|_| "AI 配置不可用")?;
        let config = config_guard.clone().ok_or("请先开启可选 AI 增强")?;
        // Capture credentials and their generation under the same configuration lock.
        let epoch = self.epoch.load(Ordering::Acquire);
        drop(config_guard);
        let config = explicit.unwrap_or(config);
        rt.command(Command::ReserveManualAi)
            .map_err(|e| e.to_string())?;
        let cancelled = || {
            self.epoch.load(Ordering::Acquire) != epoch
                || rt
                    .command(Command::Status)
                    .map(|s| s.settings.paused)
                    .unwrap_or(true)
        };
        let result = crate::assist::powershell_cancel(
            include_str!("../scripts/ai.ps1"),
            &json!({"endpoint":config.endpoint,"model":config.model,"key":config.key,"prompt":prompt}),
            cancelled,
        );
        // A fast completion can race the transport's periodic cancellation check.
        if cancelled() {
            Err("AI 请求已取消，本地助手继续运行".into())
        } else {
            result
        }
    }
    pub fn configure(
        &self,
        enabled: bool,
        endpoint: String,
        model: String,
        key: String,
    ) -> Result<(), String> {
        if enabled {
            validate(&endpoint, &model, &key)?;
        }
        let mut config = self.config.lock().map_err(|_| "AI 配置不可用")?;
        self.epoch.fetch_add(1, Ordering::AcqRel);
        *config = enabled.then_some(Config {
            endpoint,
            model,
            key,
        });
        self.set_message(if enabled {
            "AI 休眠中 · 仅陌生冲突或程序歧义需要解释时唤醒"
        } else {
            "本地模式 · AI 已关闭"
        });
        Ok(())
    }
    pub fn status(&self) -> AiStatus {
        AiStatus {
            enabled: self.config.lock().map(|c| c.is_some()).unwrap_or(false),
            awake: self.awake.load(Ordering::Acquire),
            message: self
                .message
                .lock()
                .map(|m| m.clone())
                .unwrap_or_else(|_| "本地助手继续运行".into()),
        }
    }
    fn set_message(&self, message: &str) {
        if let Ok(mut m) = self.message.lock() {
            *m = message.into();
        }
    }
    pub fn try_wake(&self, rt: &Runtime, demo: bool) {
        if demo {
            return;
        }
        let Ok(config_guard) = self.config.lock() else {
            return;
        };
        let Some(config) = config_guard.clone() else {
            return;
        };
        let epoch = self.epoch.load(Ordering::Acquire);
        drop(config_guard);
        let Ok(st) = rt.command(Command::Status) else {
            return;
        };
        if !allowed(&st) {
            return;
        }
        let Some(job) = st.experience.pending.first() else {
            return;
        };
        let Some(context) = st.current.clone() else {
            return;
        };
        // Respect feature switches: turning suggestions/adjustments off cancels their wakes too.
        if (job.kind == Kind::AppChoice && !st.settings.time_recommendations)
            || (job.kind != Kind::AppChoice && !st.settings.auto_adjustments)
        {
            return;
        }
        if self.epoch.load(Ordering::Acquire) != epoch {
            return;
        }
        if rt
            .command(Command::ClaimWake {
                id: job.id.clone(),
                revision: job.revision,
            })
            .is_err()
        {
            return;
        }
        self.awake.store(true, Ordering::Release);
        self.set_message("AI 正在了解一个陌生情况，本地助手继续工作");
        // Only names, coarse period, independent evidence count and bounded candidates leave the PC.
        // Device IDs, paths, sessions, typed text and full event histories stay local.
        let candidates: Vec<_> = job
            .candidates
            .iter()
            .enumerate()
            .map(|(i, a)| json!({"id":format!("candidate_{i}"),"name":a.name}))
            .collect();
        let prompt=json!({"kind":job.kind,"application":job.scope.app_name,"time_period":job.scope.period,
            "independent_observations":job.evidence,"candidates":candidates,
            "local_result":if job.kind==Kind::AppChoice {"多次出现接近的程序候选，需要澄清意图"}else{"用户在多个独立场景纠正调节，本地已停止在此场景自动调节；适用边界仍需判断"}}).to_string();
        let cancel = || {
            self.epoch.load(Ordering::Acquire) != epoch
                || rt
                    .command(Command::Status)
                    .map(
                        |s| {
                            !allowed(&s)
                                || s.current
                                    .as_ref()
                                    .is_none_or(|c| c.session != context.session)
                                || (job.kind == Kind::AppChoice && !s.settings.time_recommendations)
                                || (job.kind != Kind::AppChoice && !s.settings.auto_adjustments)
                                || !s.experience.pending.is_empty()
                        }, // A newer revision invalidates the in-flight job.
                    )
                    .unwrap_or(true)
        };
        let result = crate::assist::powershell_cancel(
            include_str!("../scripts/ai.ps1"),
            &json!({"endpoint":config.endpoint,"model":config.model,"key":config.key,"prompt":prompt,"learning":true}),
            cancel,
        );
        let still_current = rt.command(Command::Status).is_ok_and(|s| {
            allowed(&s)
                && s.current
                    .as_ref()
                    .is_some_and(|c| c.session == context.session)
                && if job.kind == Kind::AppChoice {
                    s.settings.time_recommendations
                } else {
                    s.settings.auto_adjustments
                }
        });
        let mut advice = if self.epoch.load(Ordering::Acquire) == epoch && still_current {
            result
                .as_ref()
                .ok()
                .and_then(|text| serde_json::from_str::<Advice>(text.trim()).ok())
        } else {
            None
        };
        if let Some(a) = &mut advice {
            if a.interpretation == Interpretation::ChooseApp {
                a.app_id = a
                    .app_id
                    .as_ref()
                    .and_then(|id| id.strip_prefix("candidate_"))
                    .and_then(|n| n.parse::<usize>().ok())
                    .and_then(|n| job.candidates.get(n))
                    .map(|a| a.id.clone());
            }
        }
        let usable = advice.is_some();
        let finished = rt
            .command(Command::FinishWake {
                id: job.id.clone(),
                revision: job.revision,
                advice,
            })
            .is_ok();
        self.awake.store(false, Ordering::Release);
        if self.epoch.load(Ordering::Acquire) == epoch {
            self.set_message(if usable && finished {
                "AI 已回到休眠 · 新经验等待确认，本地助手继续工作"
            } else {
                "AI 已回到休眠 · 未采纳本次结果，本地经验继续生效；不会循环重试"
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn optional_mode_has_no_credentials_or_network_until_enabled() {
        let layer = ColdAi::default();
        assert!(!layer.status().enabled);
        assert!(layer
            .configure(true, "http://localhost".into(), "m".into(), "k".into())
            .is_err());
        layer
            .configure(
                true,
                "https://example.com/v1/chat/completions".into(),
                "m".into(),
                "secret-key".into(),
            )
            .unwrap();
        assert!(layer.status().enabled);
        assert!(!serde_json::to_string(&layer.status())
            .unwrap()
            .contains("secret-key"));
        layer
            .configure(false, String::new(), String::new(), String::new())
            .unwrap();
        assert!(!layer.status().enabled);
        assert!(layer.config.lock().unwrap().is_none());
    }
}
