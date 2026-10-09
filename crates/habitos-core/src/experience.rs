//! Local experience owns authority. Optional AI only proposes a bounded interpretation.
//! No generated scripts, raw text input, credentials, or automatic program launches.
use crate::{model::Context, store::Store, Result};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

const LIFETIME: i64 = 30 * 86400;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Volume,
    Brightness,
    AppChoice,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Scope {
    pub app_id: String,
    pub app_name: String,
    pub device: String,
    pub period: u8,
    pub activity: String,
}
impl Scope {
    pub fn from_context(c: &Context, device: &str) -> Self {
        Self {
            app_id: c.app_id.clone(),
            app_name: c.app_name.clone(),
            device: device.into(),
            period: c.hour / 6,
            activity: c.activity.clone().unwrap_or_default(),
        }
    }
    pub fn matches(&self, other: &Self, broad: bool) -> bool {
        self.app_id == other.app_id
            && self.device == other.device
            && (broad || (self.period == other.period && self.activity == other.activity))
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppOption {
    pub id: String,
    pub name: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Interpretation {
    KeepScene,
    HoldAppDevice,
    ChooseApp,
    NoChange,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Advice {
    pub interpretation: Interpretation,
    pub app_id: Option<String>,
    pub explanation: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Experience {
    pub id: String,
    pub kind: Kind,
    pub scope: Scope,
    pub candidates: Vec<AppOption>,
    pub evidence: usize,
    pub revision: u64,
    pub last_at: i64,
    pub active: bool,
    pub broad: bool,
    pub preferred: Option<String>,
    pub wake: String,
    pub advice: Option<Advice>,
    pub awaiting_confirmation: bool,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExperienceStatus {
    pub rules: Vec<Experience>,
    pub pending: Vec<Experience>,
    pub question: Option<Experience>,
    pub local_hits: u64,
    pub automatic_calls_today: usize,
}
impl Store {
    fn experiences(&self) -> Result<Vec<Experience>> {
        let mut q = self
            .connection
            .prepare("SELECT data FROM experience_items ORDER BY rowid DESC LIMIT 500")?;
        let rows = q.query_map([], |r| r.get::<_, String>(0))?;
        rows.map(|r| Ok(serde_json::from_str(&r?)?)).collect()
    }
    pub fn experience(&self, id: &str) -> Result<Option<Experience>> {
        let row: Option<String> = self
            .connection
            .query_row("SELECT data FROM experience_items WHERE id=?1", [id], |r| {
                r.get(0)
            })
            .optional()?;
        row.map(|s| serde_json::from_str(&s).map_err(Into::into))
            .transpose()
    }
    fn save_experience(&self, e: &Experience) -> Result<()> {
        self.connection.execute(
            "UPDATE experience_items SET data=?2 WHERE id=?1",
            params![e.id, serde_json::to_string(e)?],
        )?;
        Ok(())
    }
    /// Distinct sessions are evidence, never absence of undo or our own output.
    pub fn observe_exception(
        &self,
        kind: Kind,
        scope: Scope,
        session: &str,
        mut candidates: Vec<AppOption>,
        at: i64,
    ) -> Result<Experience> {
        candidates.sort_by(|a, b| a.id.cmp(&b.id));
        candidates.dedup_by(|a, b| a.id == b.id);
        if candidates.len() > 6 || (kind == Kind::AppChoice && candidates.len() < 2) {
            return Err(crate::rule("经验候选范围无效"));
        }
        // Display names do not affect identity, nor does an individual session ID.
        let key = serde_json::to_string(&(
            kind,
            &scope.app_id,
            &scope.device,
            scope.period,
            &scope.activity,
            candidates.iter().map(|c| &c.id).collect::<Vec<_>>(),
        ))?;
        let transaction = self.connection.unchecked_transaction()?;
        self.connection.execute(
            "DELETE FROM experience_evidence WHERE at<?1",
            [at - LIFETIME],
        )?;
        let row: Option<String> = self
            .connection
            .query_row(
                "SELECT data FROM experience_items WHERE scope_key=?1",
                [&key],
                |r| r.get(0),
            )
            .optional()?;
        let mut e = if let Some(row) = row {
            serde_json::from_str::<Experience>(&row)?
        } else {
            Experience {
                id: uuid::Uuid::new_v4().to_string(),
                kind,
                scope,
                candidates,
                evidence: 0,
                revision: 0,
                last_at: at,
                active: false,
                broad: false,
                preferred: None,
                wake: "sleeping".into(),
                advice: None,
                awaiting_confirmation: false,
            }
        };
        let added = self.connection.execute(
            "INSERT OR IGNORE INTO experience_evidence VALUES(?1,?2,?3)",
            params![key, session, at],
        )?;
        if added > 0 {
            e.evidence = self.connection.query_row(
                "SELECT COUNT(*) FROM experience_evidence WHERE scope_key=?1",
                [&key],
                |r| r.get(0),
            )?;
            e.revision += 1;
            e.last_at = at;
            // A correction invalidates any outstanding interpretation.
            if e.awaiting_confirmation {
                e.awaiting_confirmation = false;
                e.advice = None;
                e.wake = "dismissed".into();
            }
            if e.wake == "claimed" {
                e.wake = "interrupted".into();
            }
            if kind != Kind::AppChoice {
                e.active = e.evidence >= 2;
            }
            if e.evidence >= 2 && e.wake == "sleeping" {
                e.wake = "pending".into();
            }
        }
        self.connection.execute("INSERT INTO experience_items VALUES(?1,?2,?3) ON CONFLICT(scope_key) DO UPDATE SET data=excluded.data",
            params![e.id,key,serde_json::to_string(&e)?])?;
        self.connection.execute("DELETE FROM experience_items WHERE id IN (SELECT id FROM experience_items ORDER BY rowid DESC LIMIT -1 OFFSET 500)",[])?;
        transaction.commit()?;
        Ok(e)
    }
    pub fn holds_adjustment(&self, kind: Kind, scope: &Scope, at: i64) -> Result<bool> {
        Ok(self.experiences()?.iter().any(|e| {
            e.kind == kind
                && e.active
                && at >= e.last_at
                && at - e.last_at < LIFETIME
                && e.scope.matches(scope, e.broad)
        }))
    }
    pub fn preferred_app(
        &self,
        scope: &Scope,
        candidates: &[String],
        at: i64,
    ) -> Result<Option<String>> {
        Ok(self
            .experiences()?
            .into_iter()
            .filter(|e| {
                e.kind == Kind::AppChoice
                    && e.active
                    && at >= e.last_at
                    && at - e.last_at < LIFETIME
                    && e.scope.matches(scope, false)
            })
            .filter_map(|e| e.preferred)
            .find(|id| candidates.contains(id)))
    }
    pub fn learn_app_selection(&self, scope: &Scope, target: &str, at: i64) -> Result<()> {
        for mut e in self.experiences()? {
            if e.kind == Kind::AppChoice
                && e.evidence >= 2
                && e.scope.matches(scope, false)
                && at >= e.last_at
                && at - e.last_at < LIFETIME
                && e.candidates.iter().any(|c| c.id == target)
            {
                e.preferred = Some(target.into());
                e.active = true;
                e.awaiting_confirmation = false;
                e.advice = None;
                e.wake = "local_resolved".into();
                e.last_at = at;
                e.revision += 1;
                self.save_experience(&e)?;
            }
        }
        Ok(())
    }
    pub fn local_experience_hit(&self) -> Result<()> {
        self.connection.execute("INSERT INTO experience_metrics VALUES(1,1) ON CONFLICT(id) DO UPDATE SET local_hits=local_hits+1",[])?;
        Ok(())
    }
    pub fn experience_status(
        &self,
        current: Option<&Context>,
        at: i64,
    ) -> Result<ExperienceStatus> {
        let rows: Vec<_> = self
            .experiences()?
            .into_iter()
            .filter(|e| at >= e.last_at && at - e.last_at < LIFETIME)
            .collect();
        let in_scene = |e: &Experience| {
            current.is_some_and(|c| {
                e.scope
                    .matches(&Scope::from_context(c, &e.scope.device), false)
            })
        };
        let question = rows
            .iter()
            .find(|e| e.awaiting_confirmation && in_scene(e))
            .cloned();
        let pending = rows
            .iter()
            .filter(|e| e.wake == "pending" && in_scene(e))
            .take(1)
            .cloned()
            .collect();
        let automatic_calls_today = self.connection.query_row(
            "SELECT COUNT(*) FROM experience_calls WHERE kind='automatic' AND at>=?1",
            [at / 86400 * 86400],
            |r| r.get(0),
        )?;
        let local_hits = self.connection.query_row(
            "SELECT COALESCE((SELECT local_hits FROM experience_metrics WHERE id=1),0)",
            [],
            |r| r.get(0),
        )?;
        Ok(ExperienceStatus {
            rules: rows.into_iter().filter(|e| e.active).collect(),
            pending,
            question,
            local_hits,
            automatic_calls_today,
        })
    }
    pub fn reserve_manual_ai(&self, at: i64) -> Result<()> {
        self.reserve_ai(at, "manual")
    }
    fn reserve_ai(&self, at: i64, kind: &str) -> Result<()> {
        let count: usize = self.connection.query_row(
            "SELECT COUNT(*) FROM experience_calls WHERE kind IN ('automatic','manual') AND at>=?1",
            [at / 86400 * 86400],
            |r| r.get(0),
        )?;
        if count >= 50 {
            return Err(crate::rule("今日 AI 请求已达上限；本地助手继续运行"));
        }
        if kind == "automatic" {
            let (count, last): (usize, Option<i64>) = self.connection.query_row(
                "SELECT COUNT(*),MAX(at) FROM experience_calls WHERE kind='automatic' AND at>=?1",
                [at / 86400 * 86400],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            if count >= 5 || last.is_some_and(|t| at - t < 600) {
                return Err(crate::rule("AI 继续休眠：自动唤醒预算或冷却中"));
            }
        }
        self.connection.execute(
            "INSERT INTO experience_calls(at,kind) VALUES(?1,?2)",
            params![at, kind],
        )?;
        self.connection.execute(
            "DELETE FROM experience_calls WHERE at>0 AND at<?1",
            [at - 90 * 86400],
        )?;
        Ok(())
    }
    pub fn claim_experience(&self, id: &str, revision: u64, at: i64) -> Result<()> {
        let mut e = self
            .experience(id)?
            .ok_or_else(|| crate::rule("经验已清除"))?;
        if e.revision != revision
            || e.wake != "pending"
            || at < e.last_at
            || at - e.last_at >= LIFETIME
        {
            return Err(crate::rule("学习场景已更新"));
        }
        let tx = self.connection.unchecked_transaction()?;
        self.reserve_ai(at, "automatic")?;
        e.wake = "claimed".into();
        self.save_experience(&e)?;
        tx.commit()?;
        Ok(())
    }
    pub fn finish_experience(
        &self,
        id: &str,
        revision: u64,
        response: Option<Advice>,
    ) -> Result<()> {
        let mut e = self
            .experience(id)?
            .ok_or_else(|| crate::rule("经验已清除"))?;
        if e.revision != revision || e.wake != "claimed" {
            return Err(crate::rule("迟到的 AI 结果已丢弃"));
        }
        e.wake = "failed".into();
        if let Some(advice) = response {
            let valid = advice.explanation.chars().count() <= 300
                && !advice.explanation.trim().is_empty()
                && !advice.explanation.chars().any(char::is_control)
                && match advice.interpretation {
                    Interpretation::ChooseApp => {
                        e.kind == Kind::AppChoice
                            && advice
                                .app_id
                                .as_ref()
                                .is_some_and(|id| e.candidates.iter().any(|c| &c.id == id))
                    }
                    Interpretation::KeepScene | Interpretation::HoldAppDevice => {
                        e.kind != Kind::AppChoice && advice.app_id.is_none()
                    }
                    Interpretation::NoChange => advice.app_id.is_none(),
                };
            if !valid {
                self.save_experience(&e)?;
                return Err(crate::rule("AI 候选超出已有能力，已忽略"));
            }
            e.awaiting_confirmation = advice.interpretation != Interpretation::NoChange;
            e.advice = Some(advice);
            e.wake = "done".into();
        }
        self.save_experience(&e)
    }
    /// Confirmation changes only a local boundary or a preferred *suggestion*.
    pub fn confirm_experience(&self, id: &str, revision: u64, accept: bool, at: i64) -> Result<()> {
        let mut e = self
            .experience(id)?
            .ok_or_else(|| crate::rule("经验已清除"))?;
        if e.revision != revision || !e.awaiting_confirmation {
            return Err(crate::rule("这条学习建议已失效"));
        }
        if accept {
            let advice = e.advice.as_ref().ok_or_else(|| crate::rule("候选不存在"))?;
            match advice.interpretation {
                Interpretation::KeepScene => e.active = true,
                Interpretation::HoldAppDevice => {
                    e.active = true;
                    e.broad = true;
                }
                Interpretation::ChooseApp => {
                    e.active = true;
                    e.preferred = advice.app_id.clone();
                }
                Interpretation::NoChange => {}
            }
            e.last_at = at;
        }
        e.awaiting_confirmation = false;
        if !accept {
            e.wake = "dismissed".into();
        }
        self.save_experience(&e)
    }
    pub fn remove_experience(&self, id: &str) -> Result<()> {
        let key: Option<String> = self
            .connection
            .query_row(
                "SELECT scope_key FROM experience_items WHERE id=?1",
                [id],
                |r| r.get(0),
            )
            .optional()?;
        let tx = self.connection.unchecked_transaction()?;
        self.connection
            .execute("DELETE FROM experience_items WHERE id=?1", [id])?;
        if let Some(key) = key {
            self.connection
                .execute("DELETE FROM experience_evidence WHERE scope_key=?1", [key])?;
        }
        tx.commit()?;
        Ok(())
    }
    pub fn clear_experience(&self) -> Result<()> {
        self.connection
            .execute_batch("DELETE FROM experience_items; DELETE FROM experience_evidence;")?;
        // Request budget survives clearing memories.
        Ok(())
    }
    pub fn forget_experience_app(&self, id: &str) -> Result<()> {
        for e in self.experiences()? {
            if e.scope.app_id == id || e.candidates.iter().any(|c| c.id == id) {
                self.remove_experience(&e.id)?;
            }
        }
        Ok(())
    }
    pub(crate) fn recover_experience(&self) -> Result<()> {
        for mut e in self.experiences()? {
            if e.wake == "claimed" {
                e.wake = "interrupted".into();
                self.save_experience(&e)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn scope() -> Scope {
        Scope {
            app_id: "editor".into(),
            app_name: "Editor.exe".into(),
            device: "speaker".into(),
            period: 3,
            activity: "application".into(),
        }
    }
    #[test]
    fn repeat_correction_works_without_ai_and_is_scoped_and_expires() {
        let s = Store::memory().unwrap();
        let c = scope();
        let one = s
            .observe_exception(Kind::Volume, c.clone(), "a", vec![], 100)
            .unwrap();
        assert!(!one.active);
        assert!(!s.holds_adjustment(Kind::Volume, &c, 101).unwrap());
        assert_eq!(
            s.observe_exception(Kind::Volume, c.clone(), "a", vec![], 110)
                .unwrap()
                .evidence,
            1
        );
        let two = s
            .observe_exception(Kind::Volume, c.clone(), "b", vec![], 120)
            .unwrap();
        assert!(two.active);
        assert!(s.holds_adjustment(Kind::Volume, &c, 121).unwrap());
        let mut different = c.clone();
        different.device = "headphone".into();
        assert!(!s.holds_adjustment(Kind::Volume, &different, 121).unwrap());
        different = c.clone();
        different.period = 0;
        assert!(!s.holds_adjustment(Kind::Volume, &different, 121).unwrap());
        assert!(!s.holds_adjustment(Kind::Brightness, &c, 121).unwrap());
        assert!(!s
            .holds_adjustment(Kind::Volume, &c, 120 + LIFETIME)
            .unwrap());
        assert_eq!(
            s.experience_status(None, 121)
                .unwrap()
                .automatic_calls_today,
            0
        );
    }
    #[test]
    fn ai_cannot_act_or_broaden_until_confirmation_and_stale_results_are_ignored() {
        let s = Store::memory().unwrap();
        let c = scope();
        s.observe_exception(Kind::Volume, c.clone(), "a", vec![], 100)
            .unwrap();
        let e = s
            .observe_exception(Kind::Volume, c.clone(), "b", vec![], 120)
            .unwrap();
        s.claim_experience(&e.id, e.revision, 1000).unwrap();
        let advice = Advice {
            interpretation: Interpretation::HoldAppDevice,
            app_id: None,
            explanation: "不同时间也保持手动设置".into(),
        };
        s.finish_experience(&e.id, e.revision, Some(advice.clone()))
            .unwrap();
        let mut other = c.clone();
        other.period = 0;
        assert!(!s.holds_adjustment(Kind::Volume, &other, 1001).unwrap());
        s.confirm_experience(&e.id, e.revision, true, 1002).unwrap();
        assert!(s.holds_adjustment(Kind::Volume, &other, 1003).unwrap());
        assert!(s
            .finish_experience(&e.id, e.revision, Some(advice))
            .is_err());
        s.remove_experience(&e.id).unwrap();
        assert!(!s.holds_adjustment(Kind::Volume, &c, 1004).unwrap());
    }
    #[test]
    fn failure_is_cached_and_budget_persists_and_response_is_bounded() {
        let s = Store::memory().unwrap();
        s.observe_exception(Kind::Brightness, scope(), "a", vec![], 100)
            .unwrap();
        let e = s
            .observe_exception(Kind::Brightness, scope(), "b", vec![], 120)
            .unwrap();
        s.claim_experience(&e.id, e.revision, 1000).unwrap();
        assert!(s
            .finish_experience(
                &e.id,
                e.revision,
                Some(Advice {
                    interpretation: Interpretation::ChooseApp,
                    app_id: Some("cmd".into()),
                    explanation: "执行命令".into()
                })
            )
            .is_err());
        assert!(s.claim_experience(&e.id, e.revision, 2000).is_err());
        assert!(s
            .holds_adjustment(Kind::Brightness, &scope(), 2001)
            .unwrap());
        s.clear_experience().unwrap();
        assert_eq!(
            s.experience_status(None, 2001)
                .unwrap()
                .automatic_calls_today,
            1
        );
    }
    #[test]
    fn persistent_budget_and_cooldown_cannot_be_bypassed_by_new_scenes_or_memory_deletion() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("memory.sqlite3");
        let s = Store::open(&path).unwrap();
        s.reserve_ai(1000, "automatic").unwrap();
        assert!(s.reserve_ai(1100, "automatic").is_err());
        for n in 1..5 {
            s.reserve_ai(1000 + n * 600, "automatic").unwrap();
        }
        assert!(s.reserve_ai(5000, "automatic").is_err());
        for _ in 0..45 {
            s.reserve_manual_ai(5000).unwrap();
        }
        assert!(s.reserve_manual_ai(5000).is_err());
        s.clear_experience().unwrap();
        drop(s);
        let s = Store::open(&path).unwrap();
        assert!(s.reserve_manual_ai(5000).is_err());
        s.reserve_manual_ai(86400).unwrap();
    }
}
