use crate::experience::{Advice, AppOption, Interpretation, Kind, Scope};
use crate::{
    learning,
    model::*,
    platform::{Platform, Snapshot},
    store::Store,
    Result,
};
use std::collections::HashMap;

pub struct Engine {
    pub store: Store,
    pub settings: Settings,
    pub current: Option<Context>,
    pub volume: Option<f64>,
    pub proposal: Option<Proposal>,
    pub app_recommendation: Option<AppRecommendation>,
    pub message: String,
    pub startup: StartupStatus,
    pub browser_last_event: i64,
    demo: bool,
    extensions: crate::extensions::Registry,
    session_started: i64,
    recent: Vec<String>,
    cooldown: HashMap<String, i64>,
    settling: Option<(f64, i64)>,
    scene_pending: bool,
    usage_pending: bool,
    last_adjustment: Option<(String, Context, f64, i64)>,
    manual_hold_session: Option<String>,
}
impl Engine {
    pub fn new(store: Store, demo: bool) -> Result<Self> {
        let settings = store.settings()?;
        store.prune(now(), settings.retention_days)?;
        Ok(Self {
            store,
            settings,
            current: None,
            volume: None,
            proposal: None,
            app_recommendation: None,
            message: "后台等待关键事件；普通场景自动学习，敏感软件单独许可".into(),
            startup: StartupStatus::default(),
            browser_last_event: 0,
            demo,
            extensions: crate::extensions::Registry::standard(),
            session_started: 0,
            recent: vec![],
            cooldown: HashMap::new(),
            settling: None,
            scene_pending: false,
            usage_pending: false,
            last_adjustment: None,
            manual_hold_session: None,
        })
    }
    pub fn observe(&mut self, snapshot: Snapshot, at: i64) -> Result<()> {
        if self.settings.paused {
            self.clear_context();
            return Ok(());
        }
        if (snapshot.audio_available && !valid_volume(snapshot.volume))
            || (snapshot.hour > 23 || snapshot.weekday > 6 || snapshot.local_day <= 0)
        {
            return Err(crate::rule("设备状态无效"));
        }
        let changed = self.current.as_ref().is_none_or(|c| {
            c.app_id != snapshot.app_id
                || c.device != snapshot.device
                || c.activity.as_deref() != Some(&snapshot.activity)
        });
        if changed {
            self.last_adjustment = None;
            self.manual_hold_session = None;
            if let Some(previous) = &self.current {
                if previous.app_id != snapshot.app_id && self.permitted(previous).is_ok() {
                    self.recent.push(previous.app_id.clone());
                }
                if self.recent.len() > 3 {
                    self.recent.remove(0);
                }
            }
            self.current = Some(Context {
                app_id: snapshot.app_id,
                app_name: snapshot.app_name,
                device: snapshot.device,
                hour: snapshot.hour,
                weekday: Some(snapshot.weekday),
                local_day: Some(snapshot.local_day),
                activity: Some(snapshot.activity),
                power: None,
                recent: self.recent.clone(),
                session: uuid::Uuid::new_v4().to_string(),
            });
            self.session_started = at;
            self.scene_pending = true;
            self.usage_pending = true;
            self.app_recommendation = None;
            self.proposal = None;
            self.settling = None;
            self.volume = snapshot.audio_available.then_some(snapshot.volume);
            self.message = "场景已切换，正在等待稳定状态".into();
        } else {
            if let Some(current) = &mut self.current {
                current.hour = snapshot.hour;
                current.weekday = Some(snapshot.weekday);
                current.local_day = Some(snapshot.local_day);
            }
            // This records external changes, not our own executor output.
            if snapshot.audio_available
                && self
                    .volume
                    .is_some_and(|v| (v - snapshot.volume).abs() > 0.015)
            {
                self.proposal = None;
                self.settling = Some((snapshot.volume, at));
                if let Some(c) = &self.current {
                    self.cooldown
                        .insert(format!("{}:{}", c.app_id, c.device), at);
                }
            }
            self.volume = snapshot.audio_available.then_some(snapshot.volume);
        }
        let current = self.current.as_ref().expect("context constructed").clone();
        self.discover(&current)?;
        if self
            .current
            .as_ref()
            .is_some_and(|c| self.permitted(c).is_err())
        {
            self.settling = None;
            self.proposal = None;
            self.scene_pending = false;
            self.usage_pending = false;
            self.app_recommendation = None;
            self.message = "该软件暂停学习，敏感软件需在面板单独允许".into();
            return Ok(());
        }
        if changed {
            let kind = if current.activity.as_deref() == Some("desktop") {
                "desktop_focus"
            } else {
                "application_focus"
            };
            self.store.event(&current, kind, at)?;
        }
        if !changed && self.settling.is_some() {
            self.manual_hold_session = Some(current.session.clone());
            if self.settings.automatic_learning {
                if let Some((_, context, after, applied)) = self.last_adjustment.take() {
                    if context.session == current.session
                        && (0..=300).contains(&(at - applied))
                        && (snapshot.volume - after).abs() > 0.015
                    {
                        self.store.observe_exception(
                            Kind::Volume,
                            Scope::from_context(&context, &context.device),
                            &context.session,
                            vec![],
                            at,
                        )?;
                        self.message = "已尊重你的纠正，本场景不再自动调音量".into();
                    }
                }
            }
        }
        if self.usage_pending && at - self.session_started >= 5 {
            self.store.record_usage(&current, at)?;
            self.usage_pending = false;
        }
        if self
            .app_recommendation
            .as_ref()
            .is_some_and(|p| at - p.created >= 60)
        {
            if let Some(p) = self.app_recommendation.take() {
                self.store.offer_status(&p.id, "expired")?;
            }
        }
        if self.proposal.as_ref().is_some_and(|p| at - p.created > 30) {
            self.proposal = None;
            if let Some(c) = &self.current {
                self.cooldown
                    .insert(format!("{}:{}", c.app_id, c.device), at);
            }
        }
        if !snapshot.audio_available {
            self.settling = None;
        }
        if let Some((value, since)) = self.settling {
            if at - since >= 3
                && at - self.session_started >= 5
                && (snapshot.volume - value).abs() < 0.015
            {
                self.capture(at, "external_change")?;
                self.settling = None;
            }
        }
        if at - self.session_started >= 3 && self.scene_pending {
            self.scene_pending = false;
            self.suggest(at)?;
            self.suggest_app(at)?;
        }
        Ok(())
    }
    pub fn clear_context(&mut self) {
        self.last_adjustment = None;
        self.manual_hold_session = None;
        self.current = None;
        self.volume = None;
        self.proposal = None;
        self.settling = None;
        self.scene_pending = false;
        self.usage_pending = false;
        self.app_recommendation = None;
    }
    fn discover(&self, c: &Context) -> Result<()> {
        if self.store.permission(&c.app_id)?.is_some() {
            return Ok(());
        }
        let sensitive = crate::policy::sensitive_app(&c.app_name);
        let observe = self.settings.automatic_learning && !sensitive;
        self.store.save_permission(&AppPermission {
            id: c.app_id.clone(),
            name: c.app_name.clone(),
            observe,
            volume: if observe { Mode::Auto } else { Mode::Off },
            sensitive,
            explicit: false,
        })
    }
    fn permitted(&self, context: &Context) -> Result<AppPermission> {
        if self.settings.paused {
            return Err(crate::rule("监听和动作已暂停"));
        }
        let p = self
            .store
            .permission(&context.app_id)?
            .ok_or_else(|| crate::rule("该软件尚未建立学习规则"))?;
        if !p.observe
            || (p.sensitive && !p.explicit)
            || (!p.explicit && !self.settings.automatic_learning)
        {
            return Err(crate::rule("该软件的观察权限已关闭"));
        }
        Ok(p)
    }
    pub fn grant_current(&mut self) -> Result<()> {
        let c = self
            .current
            .as_ref()
            .ok_or_else(|| crate::rule("尚未识别到软件"))?;
        self.store.save_permission(&AppPermission {
            id: c.app_id.clone(),
            name: c.app_name.clone(),
            observe: true,
            volume: Mode::Ask,
            sensitive: self
                .store
                .permission(&c.app_id)?
                .is_some_and(|p| p.sensitive)
                || crate::policy::sensitive_app(&c.app_name),
            explicit: true,
        })?;
        self.message = "已允许本地学习；音量动作需要逐次确认".into();
        self.scene_pending = true;
        self.usage_pending = true;
        Ok(())
    }
    pub fn permission(&mut self, id: &str, observe: bool, mode: Mode) -> Result<()> {
        if id.starts_with("web_") && mode != Mode::Off {
            return Err(crate::rule("网站没有音量调整权限"));
        }
        let mut permission = self
            .store
            .permission(id)?
            .ok_or_else(|| crate::rule("未知软件，不能直接授权"))?;
        if !observe && mode != Mode::Off {
            return Err(crate::rule("关闭观察时请同时关闭音量动作"));
        }
        permission.observe = observe;
        permission.volume = mode;
        permission.explicit = true;
        self.store.save_permission(&permission)?;
        self.proposal = None;
        self.settling = None;
        self.scene_pending = observe;
        self.usage_pending = observe;
        self.app_recommendation = None;
        Ok(())
    }
    pub fn sensitive(&mut self, id: &str, sensitive: bool) -> Result<()> {
        let mut permission = self
            .store
            .permission(id)?
            .ok_or_else(|| crate::rule("未知软件"))?;
        permission.sensitive = sensitive;
        if sensitive {
            permission.observe = false;
            permission.volume = Mode::Off;
            permission.explicit = false;
        }
        self.store.save_permission(&permission)?;
        self.proposal = None;
        self.settling = None;
        self.scene_pending = false;
        self.usage_pending = false;
        self.app_recommendation = None;
        // Already-collected history is not silently deleted; the user can clear it.
        Ok(())
    }
    pub fn log_signal(&self, kind: &str, at: i64) -> Result<()> {
        if let Some(c) = &self.current {
            if self.permitted(c).is_ok() {
                self.store.event(c, kind, at)?;
            }
        }
        Ok(())
    }
    /// Only finite one-shot deadlines. No interval-based desktop polling.
    pub fn next_deadline(&self) -> Option<i64> {
        if self.settings.paused {
            return None;
        }
        let mut deadlines = Vec::new();
        if self.scene_pending {
            deadlines.push(self.session_started + 3);
        }
        if self.usage_pending {
            deadlines.push(self.session_started + 5);
        }
        if let Some(p) = &self.app_recommendation {
            deadlines.push(p.created + 60);
        }
        if let Some((_, since)) = self.settling {
            deadlines.push((since + 3).max(self.session_started + 5));
        }
        if let Some(p) = &self.proposal {
            deadlines.push(p.created + 31);
        }
        deadlines.into_iter().min()
    }
    pub fn request_decision(&mut self) {
        if !self.settings.paused {
            self.scene_pending = true;
            self.usage_pending = true;
        }
    }
    pub fn capture(&mut self, at: i64, source: &str) -> Result<()> {
        let c = self
            .current
            .as_ref()
            .ok_or_else(|| crate::rule("没有可记录的场景"))?;
        let _ = self.permitted(c)?;
        if at - self.session_started < 5 {
            return Err(crate::rule("请在该软件停留至少 5 秒再记录"));
        }
        self.store.sample(&Sample {
            context: c.clone(),
            volume: self.volume.ok_or_else(|| crate::rule("无法读取音量"))?,
            at,
            source: source.into(),
        })?;
        self.message = "已记录该场景偏好；同一场景只计一个样本".into();
        Ok(())
    }
    fn suggest(&mut self, at: i64) -> Result<()> {
        if self.proposal.is_some() {
            return Ok(());
        }
        let Some(c) = &self.current else {
            return Ok(());
        };
        let Ok(permission) = self.permitted(c) else {
            return Ok(());
        };
        if permission.volume == Mode::Off {
            return Ok(());
        }
        if self.manual_hold_session.as_ref() == Some(&c.session)
            || self
                .store
                .holds_adjustment(Kind::Volume, &Scope::from_context(c, &c.device), at)?
        {
            self.store.local_experience_hit()?;
            self.message = "记住了，这个场景保持你手动设置的音量".into();
            return Ok(());
        }
        let key = format!("{}:{}", c.app_id, c.device);
        if self.cooldown.get(&key).is_some_and(|t| at - *t < 60) {
            return Ok(());
        }
        let samples = self.store.samples(c, at, self.settings.retention_days)?;
        let (alpha, beta) = self.store.feedback(c)?;
        let Some(mut prediction) = learning::predict(c, &samples, alpha, beta, at) else {
            self.message = "继续学习：需要至少 3 个独立场景中的稳定偏好".into();
            return Ok(());
        };
        if prediction.confidence < self.settings.suggest_threshold
            || prediction.target > self.settings.max_volume
            || self
                .volume
                .is_none_or(|v| (v - prediction.target).abs() < 0.03)
        {
            return Ok(());
        }
        if self.settings.auto_adjustments
            && !permission.sensitive
            && (permission.volume == Mode::Auto || !permission.explicit)
        {
            if let Some(v) = self.volume {
                prediction.target = v + (prediction.target - v).clamp(-0.05, 0.05);
            }
        }
        self.proposal = Some(Proposal {
            id: uuid::Uuid::new_v4().to_string(),
            context: c.clone(),
            prediction,
            created: at,
        });
        self.message = "有一条基于本地习惯的音量建议".into();
        Ok(())
    }
    pub fn auto_eligible(&self) -> Result<bool> {
        let Some(p) = &self.proposal else {
            return Ok(false);
        };
        let permission = self.permitted(&p.context)?;
        Ok(self.settings.auto_adjustments
            && self.manual_hold_session.as_ref() != Some(&p.context.session)
            && !self.store.holds_adjustment(
                Kind::Volume,
                &Scope::from_context(&p.context, &p.context.device),
                now(),
            )?
            && !permission.sensitive
            && (permission.volume == Mode::Auto
                || (!permission.explicit && permission.volume != Mode::Off))
            && p.prediction.confidence >= self.settings.auto_threshold
            && p.prediction.samples >= self.settings.min_auto_samples
            && p.prediction.sessions >= self.settings.min_auto_sessions
            && self.volume.is_some_and(|v| {
                (v - p.prediction.target).abs() <= self.settings.max_auto_delta.min(0.05) + 1e-9
            }))
    }
    fn checked_proposal(&self, id: &str, at: i64) -> Result<Proposal> {
        let p = self
            .proposal
            .as_ref()
            .filter(|p| p.id == id)
            .ok_or_else(|| crate::rule("建议已失效"))?;
        let c = self
            .current
            .as_ref()
            .ok_or_else(|| crate::rule("场景已失效"))?;
        if p.context.session != c.session
            || p.context.device != c.device
            || at - p.created > 30
            || at < p.created
        {
            return Err(crate::rule("场景已切换或建议已过期"));
        }
        let permission = self.permitted(c)?;
        if permission.volume == Mode::Off {
            return Err(crate::rule("该软件没有音量调整权限"));
        }
        Ok(p.clone())
    }
    pub fn reject(&mut self, id: &str, at: i64) -> Result<()> {
        let p = self.checked_proposal(id, at)?;
        self.store.record_feedback(&p.context, false)?;
        self.manual_hold_session = Some(p.context.session.clone());
        if self.settings.automatic_learning && !self.permitted(&p.context)?.sensitive {
            self.store.observe_exception(
                Kind::Volume,
                Scope::from_context(&p.context, &p.context.device),
                &p.context.session,
                vec![],
                at,
            )?;
        }
        self.cooldown
            .insert(format!("{}:{}", p.context.app_id, p.context.device), at);
        self.proposal = None;
        self.message = "已降低此建议的反馈权重，60 秒内不再提示".into();
        Ok(())
    }
    pub fn apply(
        &mut self,
        id: &str,
        automatic: bool,
        platform: &mut dyn Platform,
        at: i64,
    ) -> Result<()> {
        let p = self.checked_proposal(id, at)?;
        if automatic && !self.auto_eligible()? {
            return Err(crate::rule("自动执行条件不足"));
        }
        if !valid_volume(p.prediction.target) || p.prediction.target > self.settings.max_volume {
            return Err(crate::rule("目标音量超过护栏上限"));
        }
        let snapshot = platform.snapshot()?;
        if snapshot.app_id != p.context.app_id || snapshot.device != p.context.device {
            self.clear_context();
            return Err(crate::rule("前台软件或音频设备已切换"));
        }
        if self
            .volume
            .is_none_or(|v| (v - snapshot.volume).abs() > 0.02)
        {
            self.volume = snapshot.audio_available.then_some(snapshot.volume);
            self.proposal = None;
            return Err(crate::rule("音量已被外部修改，建议取消"));
        }
        let entry = JournalEntry {
            id: uuid::Uuid::new_v4().to_string(),
            app_name: p.context.app_name.clone(),
            device: p.context.device.clone(),
            before: snapshot.volume,
            after: p.prediction.target,
            status: "prepared".into(),
            at,
            undoable: false,
        };
        self.store.prepare_action(&entry, &p.context.app_id)?;
        self.cooldown
            .insert(format!("{}:{}", p.context.app_id, p.context.device), at);
        if let Err(error) = platform.set_volume(&snapshot, entry.after) {
            self.store.action_status(&entry.id, "failed")?;
            self.proposal = None;
            return Err(error);
        }
        let verified = platform.snapshot();
        if !verified.as_ref().is_ok_and(|s| {
            s.app_id == snapshot.app_id
                && s.device == entry.device
                && (s.volume - entry.after).abs() < 0.02
        }) {
            // Only restore if the same endpoint still holds our value; never overwrite a user's intervention.
            if let Ok(s) = verified {
                if s.device == entry.device
                    && s.app_id == snapshot.app_id
                    && (s.volume - entry.after).abs() < 0.02
                {
                    let _ = platform.set_volume(&s, entry.before);
                }
            }
            self.store.action_status(&entry.id, "unverified")?;
            self.proposal = None;
            return Err(crate::rule("动作未通过验证，请检查设备状态"));
        }
        if let Err(error) = self.store.action_status(&entry.id, "applied") {
            if let Ok(s) = platform.snapshot() {
                if s.device == entry.device && (s.volume - entry.after).abs() < 0.02 {
                    let _ = platform.set_volume(&s, entry.before);
                }
            }
            self.proposal = None;
            return Err(error);
        }
        self.volume = Some(entry.after);
        self.last_adjustment = Some((entry.id.clone(), p.context.clone(), entry.after, at));
        self.settling = None;
        self.proposal = None;
        self.cooldown
            .insert(format!("{}:{}", p.context.app_id, p.context.device), at);
        // Automatic actions are never recycled into training or positive feedback.
        if !automatic {
            self.store.record_feedback(&p.context, true)?;
        }
        self.message = "音量已调整；可以从动作记录撤销".into();
        Ok(())
    }
    pub fn undo(&mut self, id: &str, platform: &mut dyn Platform, at: i64) -> Result<()> {
        let entry = self
            .store
            .journal()?
            .into_iter()
            .find(|e| e.id == id)
            .ok_or_else(|| crate::rule("动作不存在"))?;
        if entry.status != "applied" || at - entry.at > 300 || at < entry.at {
            return Err(crate::rule("撤销窗口已结束（5 分钟）"));
        }
        let app_id = self.store.action_app(id)?;
        let c = self
            .current
            .as_ref()
            .ok_or_else(|| crate::rule("场景不存在"))?;
        let permission = self.permitted(c)?;
        if c.app_id != app_id || permission.volume == Mode::Off {
            return Err(crate::rule("场景或动作权限已失效"));
        }
        let snapshot = platform.snapshot()?;
        if snapshot.app_id != app_id
            || snapshot.device != entry.device
            || (snapshot.volume - entry.after).abs() > 0.02
        {
            return Err(crate::rule(
                "软件、设备或音量已变化，取消撤销以保留当前设置",
            ));
        }
        self.store.action_status(id, "undo_prepared")?;
        platform.set_volume(&snapshot, entry.before)?;
        let restored = platform.snapshot()?;
        if restored.device != entry.device || (restored.volume - entry.before).abs() > 0.02 {
            return Err(crate::rule("撤销未通过验证"));
        }
        self.store.action_status(id, "undone")?;
        let correction_context = self
            .last_adjustment
            .as_ref()
            .filter(|(action, _, _, _)| action == id)
            .map(|(_, context, _, _)| context.clone())
            .unwrap_or_else(|| c.clone());
        if self.settings.automatic_learning && !permission.sensitive {
            self.store.observe_exception(
                Kind::Volume,
                Scope::from_context(&correction_context, &entry.device),
                &correction_context.session,
                vec![],
                at,
            )?;
        }
        self.manual_hold_session = Some(c.session.clone());
        self.last_adjustment = None;
        self.volume = Some(entry.before);
        self.proposal = None;
        self.settling = None;
        self.cooldown
            .insert(format!("{}:{}", app_id, entry.device), at);
        self.message = "已恢复音量，并记住这次纠正；本场景不再自动改动".into();
        Ok(())
    }
    pub fn save_settings(&mut self, settings: Settings) -> Result<()> {
        settings.validate()?;
        // Apps discovered while the global switch was off resume with that switch.
        // Explicit denials and sensitive rules are never overwritten.
        if settings.automatic_learning && !self.settings.automatic_learning {
            for mut permission in self.store.permissions()? {
                if !permission.explicit && !permission.sensitive {
                    permission.observe = true;
                    permission.volume = Mode::Ask;
                    self.store.save_permission(&permission)?;
                }
            }
        }
        self.store.save_settings(&settings)?;
        self.store.prune(now(), settings.retention_days)?;
        self.settings = settings;
        self.proposal = None;
        self.settling = None;
        self.scene_pending = false;
        self.app_recommendation = None;
        Ok(())
    }
    pub fn forget(&mut self) -> Result<()> {
        self.store.forget()?;
        self.last_adjustment = None;
        self.manual_hold_session = None;
        self.proposal = None;
        self.settling = None;
        self.cooldown.clear();
        self.recent.clear();
        self.usage_pending = false;
        self.scene_pending = false;
        self.app_recommendation = None;
        self.message = "习惯、反馈与动作记录已清除，权限设置保留".into();
        Ok(())
    }
    pub fn demo_teach(&mut self, at: i64) -> Result<()> {
        if !self.demo {
            return Err(crate::rule("模拟样本只能写入演示数据库"));
        }
        let c = self
            .current
            .as_ref()
            .ok_or_else(|| crate::rule("演示场景不存在"))?
            .clone();
        let _ = self.permitted(&c)?;
        for index in 1..=5 {
            let mut context = c.clone();
            context.session = format!("fixture-{}", uuid::Uuid::new_v4());
            self.store.sample(&Sample {
                context,
                volume: 0.35,
                at: at - index * 3600,
                source: "demo_fixture".into(),
            })?;
        }
        self.proposal = None;
        self.cooldown.clear();
        self.suggest(at)?;
        self.message = "已加载 5 个模拟场景样本（只影响演示数据库）".into();
        Ok(())
    }
    pub fn pin_memory(&mut self, id: &str, pinned: bool) -> Result<()> {
        self.store.pin(id, pinned)
    }
    pub fn delete_memory(&mut self, id: &str) -> Result<()> {
        self.store.forget_app(id)?;
        self.recent.retain(|x| x != id);
        if let Some(c) = &mut self.current {
            c.recent.retain(|x| x != id);
        }
        self.proposal = None;
        self.app_recommendation = None;
        self.usage_pending = false;
        self.scene_pending = false;
        self.settling = None;
        self.message = "该应用的记忆与反馈已删除，权限和启动目标保留".into();
        Ok(())
    }
    fn learning_context(&self) -> Result<Context> {
        let c = self
            .current
            .as_ref()
            .ok_or_else(|| crate::rule("学习场景不存在"))?;
        if !self.settings.automatic_learning || self.permitted(c)?.sensitive {
            return Err(crate::rule("当前场景不参与 AI 学习"));
        }
        Ok(c.clone())
    }
    pub fn brightness_correction(
        &mut self,
        context: Context,
        device: String,
        at: i64,
    ) -> Result<()> {
        let fresh = self.learning_context()?;
        if fresh.session != context.session {
            return Err(crate::rule("亮度学习场景已变化"));
        }
        self.store.observe_exception(
            Kind::Brightness,
            Scope::from_context(&context, &device),
            &context.session,
            vec![],
            at,
        )?;
        Ok(())
    }
    pub fn claim_wake(&mut self, id: &str, revision: u64, at: i64) -> Result<()> {
        let c = self.learning_context()?;
        let e = self
            .store
            .experience(id)?
            .ok_or_else(|| crate::rule("经验已清除"))?;
        if !e
            .scope
            .matches(&Scope::from_context(&c, &e.scope.device), false)
            || (e.kind != Kind::Brightness && e.scope.device != c.device)
        {
            return Err(crate::rule("唤醒场景已变化"));
        }
        for option in &e.candidates {
            let allowed = self
                .store
                .permission(&option.id)?
                .is_some_and(|p| p.observe && !p.sensitive);
            if !allowed || self.store.launch_path(&option.id).is_err() {
                return Err(crate::rule("应用候选已停用"));
            }
        }
        self.store.claim_experience(id, revision, at)
    }
    pub fn finish_wake(&mut self, id: &str, revision: u64, advice: Option<Advice>) -> Result<()> {
        self.store.finish_experience(id, revision, advice)
    }
    pub fn answer_experience(
        &mut self,
        id: &str,
        revision: u64,
        accept: bool,
        platform: &mut dyn Platform,
        at: i64,
    ) -> Result<()> {
        let e = self
            .store
            .experience(id)?
            .ok_or_else(|| crate::rule("经验已清除"))?;
        let c = self.learning_context()?;
        if e.revision != revision
            || !e.awaiting_confirmation
            || at < e.last_at
            || at - e.last_at >= 30 * 86400
            || !e
                .scope
                .matches(&Scope::from_context(&c, &e.scope.device), false)
            || (e.kind != Kind::Brightness && e.scope.device != c.device)
        {
            return Err(crate::rule("学习建议场景已变化"));
        }
        if accept {
            let native = platform.snapshot()?;
            if native.app_id != c.app_id
                || native.device != c.device
                || native.activity != c.activity.clone().unwrap_or_default()
                || native.hour / 6 != c.hour / 6
            {
                return Err(crate::rule("前台场景已变化，请重新查看建议"));
            }
            if e.advice
                .as_ref()
                .is_some_and(|a| a.interpretation == Interpretation::ChooseApp)
            {
                let target = e
                    .advice
                    .as_ref()
                    .and_then(|a| a.app_id.as_ref())
                    .ok_or_else(|| crate::rule("候选不存在"))?;
                // Existing guarded launch path, after a real user click; AI never launches by itself.
                self.open_target(target, &c.session, platform, at)?;
                // The user explicitly accepted; manual launching does not train suggestion feedback.
                self.store.confirm_experience(id, revision, true, at)?;
                self.message = "记住了，下次相似场景优先在本地建议这个程序".into();
                return Ok(());
            }
        }
        self.store.confirm_experience(id, revision, accept, at)?;
        self.message = if accept {
            "记住了，下次相似场景优先使用本地经验"
        } else {
            "好的，这条 AI 学习建议已收起，不会反复询问"
        }
        .into();
        Ok(())
    }
    pub fn demo_correction(&mut self, at: i64) -> Result<()> {
        if !self.demo {
            return Err(crate::rule("仅演示模式可生成模拟纠正"));
        }
        let c = self.learning_context()?;
        for n in 0..2 {
            self.store.observe_exception(
                Kind::Volume,
                Scope::from_context(&c, &c.device),
                &format!("demo-{}-{n}", uuid::Uuid::new_v4()),
                vec![],
                at,
            )?;
        }
        self.proposal = None;
        self.message = "模拟两次独立纠正：本地已学会这个场景不自动调音量；没有调用 API".into();
        Ok(())
    }
    pub fn register_target(&mut self, path: &str) -> Result<()> {
        let file = std::path::Path::new(path);
        if !file.is_absolute()
            || !file
                .extension()
                .is_some_and(|x| x.eq_ignore_ascii_case("exe"))
        {
            return Err(crate::rule("请选择已有的本地 exe 文件"));
        }
        let metadata = file.metadata().map_err(|error| {
            crate::rule(match error.kind() {
                std::io::ErrorKind::NotFound => "所选程序不存在或已移动，请重新选择 exe 文件",
                std::io::ErrorKind::PermissionDenied => {
                    "无法读取所选程序，请选择当前账户可以访问的 exe 文件"
                }
                _ => "无法读取所选程序文件，请检查文件是否可访问",
            })
        })?;
        if !metadata.is_file() {
            return Err(crate::rule("请选择 exe 文件，不能添加文件夹"));
        }
        let name = file
            .file_name()
            .and_then(|x| x.to_str())
            .ok_or_else(|| crate::rule("程序名无效"))?;
        if crate::policy::sensitive_app(name) {
            return Err(crate::rule("敏感软件不加入自动推荐"));
        }
        let id = crate::platform::identity(path);
        if self.store.permission(&id)?.is_some_and(|p| p.sensitive) {
            return Err(crate::rule("敏感软件不加入自动推荐"));
        }
        if self.store.permission(&id)?.is_none() {
            self.store.save_permission(&AppPermission {
                id: id.clone(),
                name: name.into(),
                observe: self.settings.automatic_learning,
                volume: Mode::Ask,
                sensitive: false,
                explicit: false,
            })?;
        }
        self.store.register_target(&id, path)?;
        self.message = format!("已保存 {name}；常用应用位最多显示六个，其余可在下方列表打开");
        Ok(())
    }
    pub fn register_website(&mut self, input: &str) -> Result<()> {
        let origin = crate::website::origin(input)?;
        let id = crate::website::id(&origin);
        if self
            .store
            .permission(&id)?
            .is_some_and(|p| p.sensitive || !p.observe)
        {
            return Err(crate::rule("该网站已标为敏感或停用"));
        }
        if self.store.permission(&id)?.is_none() {
            self.store.save_permission(&AppPermission {
                id: id.clone(),
                name: origin
                    .trim_start_matches("https://")
                    .trim_end_matches('/')
                    .into(),
                observe: true,
                volume: Mode::Off,
                sensitive: false,
                explicit: true,
            })?;
        }
        self.store.register_target(&id, &origin)?;
        self.message = "已添加站点首页，点击后由默认浏览器打开".into();
        Ok(())
    }
    pub fn observe_website(&mut self, input: &str, occurred: i64, at: i64) -> Result<()> {
        if self.settings.paused
            || !self.settings.website_observation
            || occurred > at + 5
            || at - occurred > 30
        {
            return Err(crate::rule("网站观察未开启或事件已过期"));
        }
        let mut c = self
            .current
            .clone()
            .ok_or_else(|| crate::rule("等待浏览器前台事件"))?;
        if self.permitted(&c)?.sensitive || !crate::website::browser(&c.app_name) {
            return Err(crate::rule("当前不是允许观察的浏览器场景"));
        }
        let origin = crate::website::origin(input)?;
        let id = crate::website::id(&origin);
        if self
            .store
            .permission(&id)?
            .is_some_and(|p| p.sensitive || !p.observe)
        {
            return Err(crate::rule("该网站已停用观察"));
        }
        if self.store.permission(&id)?.is_none() {
            self.store.save_permission(&AppPermission {
                id: id.clone(),
                name: origin
                    .trim_start_matches("https://")
                    .trim_end_matches('/')
                    .into(),
                observe: true,
                volume: Mode::Off,
                sensitive: false,
                explicit: true,
            })?;
        }
        c.app_id = id.clone();
        c.app_name = origin
            .trim_start_matches("https://")
            .trim_end_matches('/')
            .into();
        c.activity = Some("website".into());
        self.store.record_usage(&c, at)?;
        let days = self
            .store
            .usage_scores(at, crate::usage_score::period(c.hour))?
            .into_iter()
            .find(|s| s.app_id == id)
            .map_or(0, |s| s.days);
        if days >= 3 && !self.store.targets()?.iter().any(|t| t.app_id == id) {
            self.store.register_target(&id, &origin)?;
        }
        self.browser_last_event = at;
        self.request_decision();
        self.message = "已记录站点使用节点，仅保留域名与时间摘要".into();
        Ok(())
    }
    pub fn demo_websites(&mut self, at: i64) -> Result<()> {
        if !self.demo {
            return Err(crate::rule("真实模式不接受模拟访问"));
        }
        let original = self
            .current
            .clone()
            .ok_or_else(|| crate::rule("没有场景"))?;
        let permission = self.permitted(&original)?;
        if self.settings.paused || permission.sensitive {
            return Err(crate::rule("请恢复普通场景"));
        }
        let id = "demo-browser".to_string();
        if self
            .store
            .permission(&id)?
            .is_some_and(|p| !p.observe || p.sensitive)
        {
            return Err(crate::rule("模拟浏览器已停用"));
        }
        self.store.save_permission(&AppPermission {
            id: id.clone(),
            name: "Browser.exe（模拟）".into(),
            observe: true,
            volume: Mode::Ask,
            sensitive: false,
            explicit: true,
        })?;
        self.settings.website_observation = true;
        self.store.save_settings(&self.settings)?;
        for n in (1..=3).rev() {
            let mut c = original.clone();
            c.app_id = id.clone();
            c.app_name = "Browser.exe（模拟）".into();
            c.local_day = Some(20001200 + n);
            self.current = Some(c);
            self.observe_website("https://www.bilibili.com/", at - n * 86400, at - n * 86400)?;
        }
        self.current = Some(original);
        self.message = "已加载3个独立日期的模拟Bilibili访问，不读取真实浏览器".into();
        Ok(())
    }
    pub fn target_enabled(&mut self, id: &str, enabled: bool) -> Result<()> {
        self.store.target_enabled(id, enabled)?;
        self.app_recommendation = None;
        Ok(())
    }
    pub fn remove_target(&mut self, id: &str) -> Result<()> {
        let target = self
            .store
            .targets()?
            .into_iter()
            .find(|t| t.app_id == id && t.kind == "app")
            .ok_or_else(|| crate::rule("该 exe 应用已不在已添加列表中"))?;
        self.store.remove_target(id)?;
        self.app_recommendation = None;
        self.message = format!("已从列表移除 {}，电脑上的 exe 文件保留", target.name);
        Ok(())
    }
    /// The six home launchers are NOT recommendations: registering an app must
    /// not require learning, a foreground observation or proactive suggestions.
    /// Sensitive/disabled targets remain in the management list, not the grid.
    pub fn home_apps(&self, at: i64) -> Result<Vec<RankedApp>> {
        let permissions = self.store.permissions()?;
        let current = self.current.as_ref();
        let period = current.map_or(2, |c| crate::usage_score::period(c.hour));
        let scores = self.store.usage_scores(at, period)?;
        let mut ranked = Vec::new();
        for target in self.store.targets()? {
            if !target.enabled
                || !permissions
                    .iter()
                    .any(|p| p.id == target.app_id && !p.sensitive)
            {
                continue;
            }
            let (overall, period_score, days, period_days) = scores
                .iter()
                .find(|s| s.app_id == target.app_id)
                .map_or((0.0, 0.0, 0, 0), |s| {
                    (s.overall, s.period, s.days, s.period_days)
                });
            ranked.push(RankedApp {
                kind: target.kind,
                active: current.is_some_and(|c| c.app_id == target.app_id),
                app_id: target.app_id,
                name: target.name,
                score: crate::usage_score::combine(overall, period_score, 0.0),
                overall_score: overall,
                period_score,
                association_score: 0.0,
                days,
                period_days,
            });
        }
        ranked.sort_by(|a, b| {
            b.score
                .total_cmp(&a.score)
                .then_with(|| b.period_days.cmp(&a.period_days))
                .then_with(|| a.name.cmp(&b.name))
                .then_with(|| a.app_id.cmp(&b.app_id))
        });
        ranked.truncate(6);
        Ok(ranked)
    }
    pub fn quick_apps(&self, at: i64) -> Result<Vec<RankedApp>> {
        if self.settings.paused || !self.settings.time_recommendations {
            return Ok(vec![]);
        }
        let Some(context) = &self.current else {
            return Ok(vec![]);
        };
        if self.permitted(context).is_err() || self.permitted(context)?.sensitive {
            return Ok(vec![]);
        }
        let permitted: std::collections::HashSet<String> = self
            .store
            .permissions()?
            .into_iter()
            .filter(|p| {
                p.observe && !p.sensitive && (p.explicit || self.settings.automatic_learning)
            })
            .map(|p| p.id)
            .collect();
        let rows: Vec<_> = self
            .store
            .usage(at)?
            .into_iter()
            .filter(|r| permitted.contains(&r.app_id))
            .map(|mut r| {
                if !permitted.contains(&r.previous) {
                    r.previous.clear();
                }
                r
            })
            .collect();
        let hints = if self.settings.associations {
            self.extensions.suggest(context, &rows, at)
        } else {
            vec![]
        };
        let scores = self
            .store
            .usage_scores(at, crate::usage_score::period(context.hour))?;
        let mut ranked = vec![];
        for target in self.store.targets()? {
            if !target.enabled || !permitted.contains(&target.app_id) {
                continue;
            }
            let (overall, period, days, period_days) = scores
                .iter()
                .find(|s| s.app_id == target.app_id)
                .map_or((0.0, 0.0, 0, 0), |s| {
                    (s.overall, s.period, s.days, s.period_days)
                });
            let association = hints
                .iter()
                .filter(|h| h.app_id == target.app_id)
                .map(|h| h.score)
                .fold(0.0, f64::max);
            ranked.push(RankedApp {
                kind: target.kind,
                active: target.app_id == context.app_id,
                app_id: target.app_id,
                name: target.name,
                score: crate::usage_score::combine(overall, period, association),
                overall_score: overall,
                period_score: period,
                association_score: association,
                days,
                period_days,
            });
        }
        ranked.sort_by(|a, b| {
            b.score
                .total_cmp(&a.score)
                .then_with(|| b.period_days.cmp(&a.period_days))
                .then_with(|| a.name.cmp(&b.name))
                .then_with(|| a.app_id.cmp(&b.app_id))
        });
        ranked.truncate(6);
        Ok(ranked)
    }
    pub fn open_target(
        &mut self,
        id: &str,
        _session: &str,
        platform: &mut dyn Platform,
        _at: i64,
    ) -> Result<()> {
        // A direct user click is valid even if opening the Satori window changed
        // the foreground session. The target must still be saved and enabled.
        // Explicit user click: do not route through offer/launch_app, which
        // requires proactive recommendations and would train recommendation feedback.
        let target = self
            .store
            .targets()?
            .into_iter()
            .find(|t| t.app_id == id && t.enabled)
            .ok_or_else(|| crate::rule("已添加目标不存在或已停用"))?;
        let permission = self
            .store
            .permission(id)?
            .ok_or_else(|| crate::rule("应用规则不存在"))?;
        if permission.sensitive {
            return Err(crate::rule("敏感软件受到保护，无法从 Satori 打开"));
        }
        let path = self.store.launch_path(id)?;
        if target.kind == "website" {
            platform.open_website(&crate::website::origin(&path)?)?;
        } else {
            platform.launch_program(&path)?;
        }
        self.message = if self.demo {
            if target.kind == "website" {
                "模拟打开网站，未调用真实浏览器"
            } else {
                "模拟打开应用，未启动真实程序"
            }
        } else {
            "已按你的点击打开目标；不会自动启动其他应用"
        }
        .into();
        Ok(())
    }
    fn suggest_app(&mut self, at: i64) -> Result<()> {
        if !self.settings.time_recommendations
            || self.settings.paused
            || self.app_recommendation.is_some()
        {
            return Ok(());
        }
        let Some(context) = &self.current else {
            return Ok(());
        };
        let Ok(current_permission) = self.permitted(context) else {
            return Ok(());
        };
        if current_permission.sensitive || at - self.store.last_offer()? < 1800 {
            return Ok(());
        }
        let rows = self.store.usage(at)?;
        let mut choices = Vec::new();
        for target in self.store.targets()? {
            if !target.enabled || target.app_id == context.app_id {
                continue;
            }
            let Some(p) = self.store.permission(&target.app_id)? else {
                continue;
            };
            if p.sensitive || !p.observe || (!p.explicit && !self.settings.automatic_learning) {
                continue;
            }
            if let Some(recommendation) = crate::recommendation::predict_with_support(
                context,
                &rows,
                &target.app_id,
                &target.name,
                at,
                self.store.recommendation_feedback(&target.app_id)?,
                self.settings.app_min_days,
            ) {
                choices.push(recommendation);
            }
        }
        choices.sort_by(|a, b| b.confidence.total_cmp(&a.confidence));
        if choices
            .first()
            .is_none_or(|p| p.confidence < self.settings.app_threshold)
        {
            return Ok(());
        }
        if choices.len() > 1
            && choices[0].confidence - choices[1].confidence < self.settings.app_margin
        {
            let ids: Vec<_> = choices.iter().take(6).map(|p| p.app_id.clone()).collect();
            let scope = Scope::from_context(context, &context.device);
            if let Some(preferred) = self.store.preferred_app(&scope, &ids, at)? {
                if let Some(index) = choices.iter().position(|p| p.app_id == preferred) {
                    choices.swap(0, index);
                    choices[0].reason = "按你之前确认的选择，本地推荐".into();
                    self.store.local_experience_hit()?;
                }
            } else {
                if self.settings.automatic_learning {
                    let grid = self.home_apps(at)?;
                    let candidates = choices
                        .iter()
                        .take(6)
                        .filter(|p| grid.iter().any(|a| a.app_id == p.app_id))
                        .map(|p| AppOption {
                            id: p.app_id.clone(),
                            name: p.app_name.clone(),
                        })
                        .collect::<Vec<_>>();
                    if candidates.len() >= 2 {
                        self.store.observe_exception(
                            Kind::AppChoice,
                            scope,
                            &context.session,
                            candidates,
                            at,
                        )?;
                    }
                }
                return Ok(());
            }
        }
        if let Some(p) = choices.into_iter().next() {
            // A proactive prompt must offer an app visible in the six-slot grid.
            if !self.home_apps(at)?.iter().any(|app| app.app_id == p.app_id) {
                return Ok(());
            }
            self.store.offer(&p)?;
            self.app_recommendation = Some(p);
        }
        Ok(())
    }
    fn checked_app_recommendation(&self, id: &str, at: i64) -> Result<AppRecommendation> {
        if self.settings.paused || !self.settings.time_recommendations {
            return Err(crate::rule("推荐已暂停"));
        }
        let p = self
            .app_recommendation
            .as_ref()
            .filter(|p| p.id == id && at >= p.created && at - p.created < 60)
            .ok_or_else(|| crate::rule("推荐已失效"))?;
        let permission = self
            .store
            .permission(&p.app_id)?
            .ok_or_else(|| crate::rule("应用规则不存在"))?;
        if permission.sensitive
            || !permission.observe
            || (!permission.explicit && !self.settings.automatic_learning)
        {
            return Err(crate::rule("推荐权限已关闭"));
        }
        self.store.launch_path(&p.app_id)?;
        Ok(p.clone())
    }
    pub fn dismiss_app(&mut self, id: &str, at: i64) -> Result<()> {
        let p = self.checked_app_recommendation(id, at)?;
        self.store.offer_feedback(id, &p.app_id, false)?;
        self.app_recommendation = None;
        Ok(())
    }
    pub fn launch_app(&mut self, id: &str, platform: &mut dyn Platform, at: i64) -> Result<()> {
        let p = self.checked_app_recommendation(id, at)?;
        // Recheck native foreground to prevent a card acting inside a newly sensitive scene.
        let fresh = platform.snapshot()?;
        if self
            .current
            .as_ref()
            .is_none_or(|c| c.app_id != fresh.app_id)
        {
            self.app_recommendation = None;
            return Err(crate::rule("前台场景已变化，请等待新推荐"));
        }
        let current = self
            .current
            .as_ref()
            .ok_or_else(|| crate::rule("场景已失效"))?;
        if self.permitted(current)?.sensitive {
            return Err(crate::rule("敏感场景不打开推荐"));
        }
        let selection_scope = Scope::from_context(current, &current.device);
        let path = self.store.launch_path(&p.app_id)?;
        self.store.offer_status(id, "launch_prepared")?;
        let result = if p.app_id.starts_with("web_") {
            platform.open_website(&crate::website::origin(&path)?)
        } else {
            platform.launch_program(&path)
        };
        if let Err(error) = result {
            self.store.offer_status(id, "failed")?;
            self.app_recommendation = None;
            return Err(error);
        }
        self.store.offer_feedback(id, &p.app_id, true)?;
        if self.settings.automatic_learning {
            self.store
                .learn_app_selection(&selection_scope, &p.app_id, at)?;
        }
        self.app_recommendation = None;
        self.message = if self.demo && p.app_id.starts_with("web_") {
            "模拟打开网站，未调用真实浏览器"
        } else if self.demo {
            "模拟打开应用，未启动真实程序"
        } else {
            "已启动所选应用；不会自动关闭或代替你操作"
        }
        .into();
        Ok(())
    }
    pub fn demo_recommend(&mut self, at: i64) -> Result<()> {
        if !self.demo {
            return Err(crate::rule("仅演示模式可加载模拟推荐"));
        }
        let c = self
            .current
            .as_ref()
            .ok_or_else(|| crate::rule("没有场景"))?
            .clone();
        if self.permitted(&c)?.sensitive {
            return Err(crate::rule("敏感场景不加载推荐"));
        }
        let id = crate::platform::identity("demo:editor.exe");
        if c.app_id == id {
            return Err(crate::rule("请先切换到游戏或桌面场景"));
        }
        if self.store.permission(&id)?.is_none() {
            self.store.save_permission(&AppPermission {
                id: id.clone(),
                name: "Editor.exe（模拟）".into(),
                observe: true,
                volume: Mode::Ask,
                sensitive: false,
                explicit: false,
            })?;
        }
        self.store.register_target(&id, "demo:editor.exe")?;
        for day in (1..=8).rev() {
            let mut sample = c.clone();
            sample.app_id = id.clone();
            sample.app_name = "Editor.exe（模拟）".into();
            sample.activity = Some("application".into());
            sample.local_day = Some(20000100 + day);
            sample.recent = vec![c.app_id.clone()];
            self.store.record_usage(&sample, at - day * 7 * 86400)?;
        }
        self.app_recommendation = None;
        self.suggest_app(at)?;
        self.message = "已加载 8 个独立日期的模拟推荐样本".into();
        Ok(())
    }
    pub fn demo_grid(&mut self, at: i64) -> Result<()> {
        if !self.demo {
            return Err(crate::rule("仅演示模式可加载示例应用"));
        }
        let context = self
            .current
            .clone()
            .ok_or_else(|| crate::rule("没有场景"))?;
        if self.permitted(&context)?.sensitive {
            return Err(crate::rule("敏感场景不加载示例"));
        }
        for (index, name) in ["Editor", "Browser", "Notes", "Music", "Terminal", "Video"]
            .iter()
            .enumerate()
        {
            let path = format!("demo:{}.exe", name.to_lowercase());
            let id = crate::platform::identity(&path);
            if self.store.permission(&id)?.is_none() {
                self.store.save_permission(&AppPermission {
                    id: id.clone(),
                    name: format!("{name}.exe（模拟）"),
                    observe: true,
                    volume: Mode::Ask,
                    sensitive: false,
                    explicit: false,
                })?;
            }
            if self
                .store
                .permission(&id)?
                .is_some_and(|p| p.sensitive || !p.observe)
            {
                continue;
            }
            self.store.register_target(&id, &path)?;
            for day in (1..=8).rev() {
                let mut c = context.clone();
                c.app_id = id.clone();
                c.hour = if index < 4 {
                    context.hour
                } else {
                    (context.hour + 12) % 24
                };
                c.activity = Some("application".into());
                c.local_day = Some(20001000 + day);
                c.recent = if index < 3 {
                    vec![context.app_id.clone()]
                } else {
                    vec![]
                };
                self.store.record_usage(&c, at - day * 86400)?;
            }
        }
        self.message = "已加载6个模拟应用".into();
        Ok(())
    }
    pub fn status(&self, at: i64) -> Result<Status> {
        let mut journal = self.store.journal()?;
        for e in &mut journal {
            let app_id = self.store.action_app(&e.id)?;
            e.undoable = e.status == "applied"
                && at >= e.at
                && at - e.at <= 300
                && !self.settings.paused
                && self
                    .current
                    .as_ref()
                    .is_some_and(|c| c.device == e.device && c.app_id == app_id)
                && self
                    .store
                    .permission(&app_id)?
                    .is_some_and(|p| p.observe && p.volume != Mode::Off)
                && self.volume.is_some_and(|v| (v - e.after).abs() <= 0.02);
        }
        Ok(Status {
            experience: self.store.experience_status(
                self.current.as_ref().filter(|c| {
                    self.settings.automatic_learning
                        && self.permitted(c).is_ok_and(|p| !p.sensitive)
                }),
                at,
            )?,
            demo: self.demo,
            quick_apps: self.quick_apps(at)?,
            home_apps: self.home_apps(at)?,
            capabilities: crate::extensions::capabilities(self.demo || cfg!(windows)),
            extensions: self.extensions.info(),
            browser_last_event: self.browser_last_event,
            startup: self.startup.clone(),
            settings: self.settings.clone(),
            current: self.current.clone(),
            volume: self.volume,
            apps: self.store.permissions()?,
            proposal: self
                .proposal
                .clone()
                .filter(|p| at >= p.created && at - p.created <= 30),
            journal,
            samples: self.store.count_samples()?,
            events: self.store.events()?,
            event_count: self.store.event_count()?,
            message: self.message.clone(),
            memories: self.store.memories(at)?,
            targets: self.store.targets()?,
            app_recommendation: self
                .app_recommendation
                .clone()
                .filter(|p| at >= p.created && at - p.created < 60),
        })
    }
}
