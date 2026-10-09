use crate::{model::*, Result};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;

pub struct Store {
    pub(crate) connection: Connection,
}
impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        Self::from_connection(Connection::open(path)?)
    }
    pub fn memory() -> Result<Self> {
        Self::from_connection(Connection::open_in_memory()?)
    }
    fn from_connection(connection: Connection) -> Result<Self> {
        connection.busy_timeout(std::time::Duration::from_secs(2))?;
        connection.execute_batch(
            "PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON; PRAGMA synchronous=FULL;",
        )?;
        let version: i64 = connection.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version > 5 {
            return Err(crate::rule("数据库来自更新版本，请先升级程序"));
        }
        connection.execute_batch("BEGIN IMMEDIATE;
            CREATE TABLE IF NOT EXISTS apps(id TEXT PRIMARY KEY, name TEXT NOT NULL, observe INTEGER NOT NULL, mode TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS samples(app_id TEXT NOT NULL REFERENCES apps(id) ON DELETE CASCADE,
                device TEXT NOT NULL, session TEXT NOT NULL, context TEXT NOT NULL, volume REAL NOT NULL,
                at INTEGER NOT NULL, source TEXT NOT NULL, PRIMARY KEY(app_id,device,session));
            CREATE INDEX IF NOT EXISTS samples_lookup ON samples(app_id,device,at);
            CREATE TABLE IF NOT EXISTS feedback(app_id TEXT NOT NULL REFERENCES apps(id) ON DELETE CASCADE,
                device TEXT NOT NULL, alpha REAL NOT NULL, beta REAL NOT NULL, PRIMARY KEY(app_id,device));
            CREATE TABLE IF NOT EXISTS settings(id INTEGER PRIMARY KEY CHECK(id=1), data TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS journal(id TEXT PRIMARY KEY, app_id TEXT NOT NULL, app_name TEXT NOT NULL,
                device TEXT NOT NULL, before REAL NOT NULL, after REAL NOT NULL, status TEXT NOT NULL, at INTEGER NOT NULL);
            COMMIT;")?;
        if version < 2 {
            connection.execute_batch(
                "BEGIN IMMEDIATE;
                ALTER TABLE apps ADD COLUMN sensitive INTEGER NOT NULL DEFAULT 0;
                ALTER TABLE apps ADD COLUMN explicit INTEGER NOT NULL DEFAULT 1;
                CREATE TABLE behavior_events(id INTEGER PRIMARY KEY, app_id TEXT NOT NULL,
                    app_name TEXT NOT NULL, kind TEXT NOT NULL, at INTEGER NOT NULL);
                CREATE INDEX behavior_time ON behavior_events(at);
                PRAGMA user_version=2; COMMIT;",
            )?;
        }
        if version < 3 {
            connection.execute_batch("BEGIN IMMEDIATE;
                CREATE TABLE usage_days(app_id TEXT NOT NULL REFERENCES apps(id),local_day INTEGER NOT NULL,
                    hour INTEGER NOT NULL,weekday INTEGER NOT NULL,previous TEXT NOT NULL,at INTEGER NOT NULL,
                    PRIMARY KEY(app_id,local_day,hour));
                CREATE TABLE memory_meta(app_id TEXT PRIMARY KEY REFERENCES apps(id),pinned INTEGER NOT NULL DEFAULT 0,
                    last_used INTEGER NOT NULL,strength REAL NOT NULL DEFAULT 0.1,days INTEGER NOT NULL DEFAULT 0);
                CREATE TABLE launch_targets(app_id TEXT PRIMARY KEY REFERENCES apps(id),path TEXT NOT NULL,enabled INTEGER NOT NULL DEFAULT 1);
                CREATE TABLE recommendation_feedback(app_id TEXT PRIMARY KEY,alpha REAL NOT NULL,beta REAL NOT NULL);
                CREATE TABLE offers(id TEXT PRIMARY KEY,app_id TEXT NOT NULL,status TEXT NOT NULL,at INTEGER NOT NULL);
                INSERT INTO memory_meta(app_id,last_used,strength,days) SELECT app_id,MAX(at),0.2,COUNT(DISTINCT at/86400) FROM samples GROUP BY app_id;
                PRAGMA user_version=3; COMMIT;")?;
        }
        if version < 4 {
            connection.execute_batch("BEGIN IMMEDIATE;
                CREATE TABLE app_scores(app_id TEXT PRIMARY KEY REFERENCES apps(id),strength REAL NOT NULL,last_used INTEGER NOT NULL,days INTEGER NOT NULL);
                CREATE TABLE period_scores(app_id TEXT NOT NULL REFERENCES apps(id),period INTEGER NOT NULL,strength REAL NOT NULL,last_used INTEGER NOT NULL,days INTEGER NOT NULL,PRIMARY KEY(app_id,period));")?;
            let overall: Vec<(String, i64, i64)> = {
                let mut q=connection.prepare("SELECT app_id,COUNT(DISTINCT local_day),MAX(at) FROM usage_days WHERE hour BETWEEN 0 AND 23 GROUP BY app_id")?;
                let rows = q.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
                rows.collect::<std::result::Result<Vec<_>, _>>()?
            };
            for (id, days, last) in overall {
                connection.execute(
                    "INSERT INTO app_scores VALUES(?1,?2,?3,?4)",
                    params![
                        id,
                        1.0 - (1.0 - crate::usage_score::OVERALL_RATE).powf(days as f64),
                        last,
                        days
                    ],
                )?;
            }
            for part in 0..6 {
                let (start, end) = crate::usage_score::bounds(part);
                let rows: Vec<(String, i64, i64)> = {
                    let mut q=connection.prepare("SELECT app_id,COUNT(DISTINCT local_day),MAX(at) FROM usage_days WHERE hour>=?1 AND hour<?2 GROUP BY app_id")?;
                    let rows = q.query_map(params![start, end], |r| {
                        Ok((r.get(0)?, r.get(1)?, r.get(2)?))
                    })?;
                    rows.collect::<std::result::Result<Vec<_>, _>>()?
                };
                for (id, days, last) in rows {
                    connection.execute(
                        "INSERT INTO period_scores VALUES(?1,?2,?3,?4,?5)",
                        params![
                            id,
                            part,
                            1.0 - (1.0 - crate::usage_score::PERIOD_RATE).powf(days as f64),
                            last,
                            days
                        ],
                    )?;
                }
            }
            connection.execute_batch("PRAGMA user_version=4; COMMIT;")?;
        }
        connection.execute_batch("BEGIN IMMEDIATE;
            CREATE TABLE IF NOT EXISTS experience_items(id TEXT PRIMARY KEY, scope_key TEXT UNIQUE NOT NULL, data TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS experience_evidence(scope_key TEXT NOT NULL, session TEXT NOT NULL, at INTEGER NOT NULL, PRIMARY KEY(scope_key,session));
            CREATE TABLE IF NOT EXISTS experience_calls(id INTEGER PRIMARY KEY, at INTEGER NOT NULL, kind TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS experience_metrics(id INTEGER PRIMARY KEY CHECK(id=1), local_hits INTEGER NOT NULL);
            CREATE INDEX IF NOT EXISTS experience_calls_time ON experience_calls(at);
            PRAGMA user_version=5; COMMIT;")?;
        // An interrupted wake is not retried automatically after restart.
        let mut store = Self { connection };
        store.recover_experience()?;
        let connection = &mut store.connection;
        // An interrupted operation must never be silently advertised as undoable.
        connection.execute(
            "UPDATE journal SET status='interrupted' WHERE status='prepared'",
            [],
        )?;
        connection.execute(
            "UPDATE journal SET status='undo_interrupted' WHERE status='undo_prepared'",
            [],
        )?;
        Ok(store)
    }
    pub fn settings(&self) -> Result<Settings> {
        let data: Option<String> = self
            .connection
            .query_row("SELECT data FROM settings WHERE id=1", [], |r| r.get(0))
            .optional()?;
        let settings = match data {
            Some(data) => {
                let old: serde_json::Value = serde_json::from_str(&data)?;
                let mut s: Settings = serde_json::from_value(old.clone())?;
                if old.get("auto_adjustments").is_none() {
                    s.app_min_days = 2;
                    s.app_threshold = 0.50;
                    s.suggest_threshold = 0.50;
                    s.auto_threshold = 0.50;
                    s.min_auto_samples = 3;
                    s.min_auto_sessions = 3;
                    s.max_auto_delta = 0.05;
                }
                s
            }
            None => Settings::default(),
        };
        settings.validate()?;
        Ok(settings)
    }
    pub fn save_settings(&self, settings: &Settings) -> Result<()> {
        settings.validate()?;
        self.connection.execute(
            "INSERT INTO settings VALUES(1,?1) ON CONFLICT(id) DO UPDATE SET data=excluded.data",
            [serde_json::to_string(settings)?],
        )?;
        Ok(())
    }
    pub fn permission(&self, id: &str) -> Result<Option<AppPermission>> {
        let data: Option<(String, bool, String, bool, bool)> = self
            .connection
            .query_row(
                "SELECT name,observe,mode,sensitive,explicit FROM apps WHERE id=?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .optional()?;
        data.map(|(name, observe, mode, sensitive, explicit)| {
            Ok(AppPermission {
                id: id.into(),
                name,
                observe,
                volume: serde_json::from_str(&mode)?,
                sensitive,
                explicit,
            })
        })
        .transpose()
    }
    pub fn permissions(&self) -> Result<Vec<AppPermission>> {
        let mut statement = self
            .connection
            .prepare("SELECT id FROM apps ORDER BY name")?;
        let ids = statement
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        ids.iter()
            .map(|id| {
                self.permission(id)?
                    .ok_or_else(|| crate::rule("应用权限缺失"))
            })
            .collect()
    }
    pub fn save_permission(&self, app: &AppPermission) -> Result<()> {
        self.connection.execute("INSERT INTO apps VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(id) DO UPDATE SET observe=excluded.observe,mode=excluded.mode,sensitive=excluded.sensitive,explicit=excluded.explicit",
            params![app.id,app.name,app.observe,serde_json::to_string(&app.volume)?,app.sensitive,app.explicit])?;
        Ok(())
    }
    pub fn sample(&self, sample: &Sample) -> Result<()> {
        if !valid_volume(sample.volume) {
            return Err(crate::rule("音量值无效"));
        }
        let tx = self.connection.unchecked_transaction()?;
        tx.execute("INSERT INTO samples VALUES(?1,?2,?3,?4,?5,?6,?7)
            ON CONFLICT(app_id,device,session) DO UPDATE SET context=excluded.context,volume=excluded.volume,at=excluded.at,source=excluded.source",
            params![sample.context.app_id,sample.context.device,sample.context.session,
                serde_json::to_string(&sample.context)?, sample.volume,sample.at,sample.source])?;
        tx.execute(
            "INSERT INTO memory_meta(app_id,last_used,strength) VALUES(?1,?2,0.2)
            ON CONFLICT(app_id) DO UPDATE SET last_used=MAX(last_used,excluded.last_used)",
            params![sample.context.app_id, sample.at],
        )?;
        tx.commit()?;
        Ok(())
    }
    pub fn samples(&self, context: &Context, now: i64, retention_days: i64) -> Result<Vec<Sample>> {
        let mut statement = self.connection.prepare("SELECT context,volume,at,source FROM samples WHERE app_id=?1 AND device=?2 AND at>=?3 ORDER BY at DESC LIMIT 100")?;
        let rows = statement.query_map(
            params![context.app_id, context.device, now - retention_days * 86400],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, f64>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, String>(3)?,
                ))
            },
        )?;
        rows.map(|r| {
            let (context, volume, at, source) = r?;
            Ok(Sample {
                context: serde_json::from_str(&context)?,
                volume,
                at,
                source,
            })
        })
        .collect()
    }
    pub fn count_samples(&self) -> Result<usize> {
        Ok(self
            .connection
            .query_row("SELECT COUNT(*) FROM samples", [], |r| r.get(0))?)
    }
    pub fn feedback(&self, context: &Context) -> Result<(f64, f64)> {
        Ok(self
            .connection
            .query_row(
                "SELECT alpha,beta FROM feedback WHERE app_id=?1 AND device=?2",
                params![context.app_id, context.device],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?
            .unwrap_or((1.0, 1.0)))
    }
    pub fn record_feedback(&self, context: &Context, accepted: bool) -> Result<()> {
        let (mut alpha, mut beta) = self.feedback(context)?;
        if accepted {
            alpha += 1.0;
        } else {
            beta += 1.0;
        }
        if alpha + beta > 50.0 {
            let scale = 48.0 / (alpha + beta - 2.0);
            alpha = 1.0 + (alpha - 1.0) * scale;
            beta = 1.0 + (beta - 1.0) * scale;
        }
        self.connection.execute("INSERT INTO feedback VALUES(?1,?2,?3,?4) ON CONFLICT(app_id,device) DO UPDATE SET alpha=excluded.alpha,beta=excluded.beta",
            params![context.app_id,context.device,alpha,beta])?;
        Ok(())
    }
    pub fn prepare_action(&self, entry: &JournalEntry, app_id: &str) -> Result<()> {
        self.connection.execute(
            "INSERT INTO journal VALUES(?1,?2,?3,?4,?5,?6,'prepared',?7)",
            params![
                entry.id,
                app_id,
                entry.app_name,
                entry.device,
                entry.before,
                entry.after,
                entry.at
            ],
        )?;
        Ok(())
    }
    pub fn action_status(&self, id: &str, status: &str) -> Result<()> {
        self.connection.execute(
            "UPDATE journal SET status=?2 WHERE id=?1",
            params![id, status],
        )?;
        Ok(())
    }
    pub fn journal(&self) -> Result<Vec<JournalEntry>> {
        let mut statement = self.connection.prepare("SELECT id,app_name,device,before,after,status,at FROM journal ORDER BY at DESC,rowid DESC LIMIT 50")?;
        let rows = statement.query_map([], |r| {
            Ok(JournalEntry {
                id: r.get(0)?,
                app_name: r.get(1)?,
                device: r.get(2)?,
                before: r.get(3)?,
                after: r.get(4)?,
                status: r.get(5)?,
                at: r.get(6)?,
                undoable: false,
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }
    pub fn action_app(&self, id: &str) -> Result<String> {
        Ok(self
            .connection
            .query_row("SELECT app_id FROM journal WHERE id=?1", [id], |r| r.get(0))?)
    }
    pub fn prune(&self, at: i64, days: i64) -> Result<()> {
        let cutoff = at - days * 86400;
        for m in self.memories(at)? {
            if !m.pinned && m.last_used < cutoff && m.importance < 0.12 {
                self.forget_app(&m.app_id)?;
            } else if !m.pinned && m.importance < 0.35 {
                self.connection.execute(
                    "DELETE FROM samples WHERE app_id=?1 AND at<?2",
                    params![m.app_id, cutoff],
                )?;
                self.connection.execute(
                    "DELETE FROM usage_days WHERE app_id=?1 AND at<?2",
                    params![m.app_id, cutoff],
                )?;
            }
            // Pinning preserves useful learning, not an unbounded raw event log.
            self.connection.execute("DELETE FROM samples WHERE app_id=?1 AND rowid NOT IN (SELECT rowid FROM samples WHERE app_id=?1 ORDER BY at DESC LIMIT 200)", [&m.app_id])?;
            self.connection.execute("DELETE FROM usage_days WHERE app_id=?1 AND rowid NOT IN (SELECT rowid FROM usage_days WHERE app_id=?1 ORDER BY at DESC LIMIT 400)", [&m.app_id])?;
        }
        // Old databases can contain samples without a memory card; keep their retention guard.
        self.connection.execute(
            "DELETE FROM samples WHERE at<?1 AND app_id NOT IN (SELECT app_id FROM memory_meta)",
            [cutoff],
        )?;
        self.connection
            .execute("DELETE FROM journal WHERE at<?1", [cutoff])?;
        self.connection
            .execute("DELETE FROM behavior_events WHERE at<?1", [cutoff])?;
        self.connection
            .execute("DELETE FROM offers WHERE at<?1", [cutoff])?;
        Ok(())
    }
    pub fn forget(&mut self) -> Result<()> {
        let tx = self.connection.transaction()?;
        tx.execute_batch("DELETE FROM samples; DELETE FROM feedback; DELETE FROM journal; DELETE FROM behavior_events; DELETE FROM usage_days; DELETE FROM memory_meta; DELETE FROM recommendation_feedback; DELETE FROM offers; DELETE FROM app_scores; DELETE FROM period_scores;")?;
        tx.commit()?;
        self.clear_experience()?;
        Ok(())
    }
    pub fn event(&self, c: &Context, kind: &str, at: i64) -> Result<()> {
        self.connection.execute(
            "INSERT INTO behavior_events(app_id,app_name,kind,at) VALUES(?1,?2,?3,?4)",
            params![c.app_id, c.app_name, kind, at],
        )?;
        Ok(())
    }
    pub fn event_count(&self) -> Result<usize> {
        Ok(self
            .connection
            .query_row("SELECT COUNT(*) FROM behavior_events", [], |r| r.get(0))?)
    }
    pub fn events(&self) -> Result<Vec<BehaviorEvent>> {
        let mut statement = self.connection.prepare(
            "SELECT app_name,kind,at FROM behavior_events ORDER BY at DESC,id DESC LIMIT 50",
        )?;
        let rows = statement.query_map([], |r| {
            Ok(BehaviorEvent {
                app_name: r.get(0)?,
                kind: r.get(1)?,
                at: r.get(2)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn record_usage(&self, c: &Context, at: i64) -> Result<()> {
        let (Some(local_day), Some(weekday)) = (c.local_day, c.weekday) else {
            return Ok(());
        };
        if weekday > 6
            || c.hour > 23
            || !matches!(c.activity.as_deref(), Some("application" | "website"))
        {
            return Ok(());
        }
        let seen: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM usage_days WHERE app_id=?1 AND local_day=?2)",
            params![c.app_id, local_day],
            |r| r.get(0),
        )?;
        let part = crate::usage_score::period(c.hour);
        let (start, end) = crate::usage_score::bounds(part);
        let period_seen: bool=self.connection.query_row("SELECT EXISTS(SELECT 1 FROM usage_days WHERE app_id=?1 AND local_day=?2 AND hour>=?3 AND hour<?4)",params![c.app_id,local_day,start,end],|r|r.get(0))?;
        let global: Option<(f64, i64)> = self
            .connection
            .query_row(
                "SELECT strength,last_used FROM app_scores WHERE app_id=?1",
                [&c.app_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let local: Option<(f64, i64)> = self
            .connection
            .query_row(
                "SELECT strength,last_used FROM period_scores WHERE app_id=?1 AND period=?2",
                params![c.app_id, part],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let global_strength = global.map_or(0.0, |(strength, last)| {
            crate::usage_score::decay(strength, last, at)
        });
        let period_strength = local.map_or(0.0, |(strength, last)| {
            crate::usage_score::decay(strength, last, at)
        });
        let tx = self.connection.unchecked_transaction()?;
        tx.execute("INSERT INTO usage_days VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(app_id,local_day,hour) DO UPDATE SET at=MAX(at,excluded.at)",
            params![c.app_id,local_day,c.hour,weekday,c.recent.last().cloned().unwrap_or_default(),at])?;
        tx.execute("INSERT INTO memory_meta(app_id,last_used) VALUES(?1,?2) ON CONFLICT(app_id) DO UPDATE SET last_used=MAX(last_used,excluded.last_used)",params![c.app_id,at])?;
        if !seen {
            tx.execute("UPDATE memory_meta SET strength=strength+?2*(1.0-strength),days=days+1 WHERE app_id=?1",params![c.app_id,crate::memory::LEARNING_RATE])?;
        }
        tx.execute("INSERT INTO app_scores VALUES(?1,?3,?2,0) ON CONFLICT(app_id) DO UPDATE SET strength=excluded.strength,last_used=MAX(last_used,excluded.last_used)",params![c.app_id,at,global_strength])?;
        tx.execute("INSERT INTO period_scores VALUES(?1,?2,?4,?3,0) ON CONFLICT(app_id,period) DO UPDATE SET strength=excluded.strength,last_used=MAX(last_used,excluded.last_used)",params![c.app_id,part,at,period_strength])?;
        if !seen {
            tx.execute("UPDATE app_scores SET strength=strength+?2*(1.0-strength),days=days+1 WHERE app_id=?1",params![c.app_id,crate::usage_score::OVERALL_RATE])?;
        }
        if !period_seen {
            tx.execute("UPDATE period_scores SET strength=strength+?3*(1.0-strength),days=days+1 WHERE app_id=?1 AND period=?2",params![c.app_id,part,crate::usage_score::PERIOD_RATE])?;
        }
        tx.commit()?;
        Ok(())
    }
    pub fn usage_scores(&self, at: i64, part: u8) -> Result<Vec<UsageScore>> {
        let mut q=self.connection.prepare("SELECT a.app_id,a.strength,a.last_used,a.days,COALESCE(p.strength,0),COALESCE(p.last_used,?2),COALESCE(p.days,0) FROM app_scores a LEFT JOIN period_scores p ON p.app_id=a.app_id AND p.period=?1")?;
        let rows = q.query_map(params![part, at], |r| {
            Ok(UsageScore {
                app_id: r.get(0)?,
                overall: crate::usage_score::decay(r.get(1)?, r.get(2)?, at),
                days: r.get(3)?,
                period: crate::usage_score::decay(r.get(4)?, r.get(5)?, at),
                period_days: r.get(6)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }
    pub fn usage(&self, at: i64) -> Result<Vec<Usage>> {
        let mut q=self.connection.prepare("SELECT app_id,local_day,hour,weekday,previous,at FROM usage_days WHERE at>=?1 AND at<=?2 ORDER BY at DESC LIMIT 4000")?;
        let rows = q.query_map(params![at - 63 * 86400, at], |r| {
            Ok(Usage {
                app_id: r.get(0)?,
                local_day: r.get(1)?,
                hour: r.get(2)?,
                weekday: r.get(3)?,
                previous: r.get(4)?,
                at: r.get(5)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }
    pub fn memories(&self, at: i64) -> Result<Vec<MemoryItem>> {
        let mut q=self.connection.prepare("SELECT m.app_id,a.name,m.pinned,m.last_used,m.strength,m.days,(SELECT COUNT(*) FROM samples s WHERE s.app_id=m.app_id) FROM memory_meta m JOIN apps a ON a.id=m.app_id ORDER BY m.pinned DESC,m.last_used DESC")?;
        let rows = q.query_map([], |r| {
            let pinned = r.get(2)?;
            let last_used = r.get(3)?;
            Ok(MemoryItem {
                app_id: r.get(0)?,
                name: r.get(1)?,
                pinned,
                last_used,
                importance: crate::memory::importance(r.get(4)?, last_used, at, pinned),
                days: r.get(5)?,
                samples: r.get(6)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }
    pub fn pin(&self, id: &str, pinned: bool) -> Result<()> {
        if self.connection.execute(
            "UPDATE memory_meta SET pinned=?2 WHERE app_id=?1",
            params![id, pinned],
        )? == 0
        {
            return Err(crate::rule("记忆不存在"));
        }
        Ok(())
    }
    pub fn forget_app(&self, id: &str) -> Result<()> {
        self.forget_experience_app(id)?;
        let tx = self.connection.unchecked_transaction()?;
        for table in [
            "samples",
            "feedback",
            "journal",
            "behavior_events",
            "usage_days",
            "memory_meta",
            "recommendation_feedback",
            "offers",
            "app_scores",
            "period_scores",
        ] {
            tx.execute(&format!("DELETE FROM {table} WHERE app_id=?1"), [id])?;
        }
        tx.execute("UPDATE usage_days SET previous='' WHERE previous=?1", [id])?;
        // Remove the forgotten identity from other persisted scene histories too.
        let contexts: Vec<(i64, String)> = {
            let mut q = tx.prepare("SELECT rowid,context FROM samples")?;
            let rows = q.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
            rows.collect::<std::result::Result<Vec<_>, _>>()?
        };
        for (rowid, json) in contexts {
            let mut context: Context = serde_json::from_str(&json)?;
            if context.recent.contains(&id.to_string()) {
                context.recent.retain(|x| x != id);
                tx.execute(
                    "UPDATE samples SET context=?2 WHERE rowid=?1",
                    params![rowid, serde_json::to_string(&context)?],
                )?;
            }
        }
        tx.commit()?;
        Ok(())
    }
    pub fn register_target(&self, id: &str, path: &str) -> Result<()> {
        self.connection.execute("INSERT INTO launch_targets VALUES(?1,?2,1) ON CONFLICT(app_id) DO UPDATE SET path=excluded.path,enabled=1",params![id,path])?;
        Ok(())
    }
    pub fn targets(&self) -> Result<Vec<LaunchTarget>> {
        let mut q=self.connection.prepare("SELECT t.app_id,a.name,t.enabled FROM launch_targets t JOIN apps a ON a.id=t.app_id ORDER BY a.name")?;
        let rows = q.query_map([], |r| {
            Ok(LaunchTarget {
                kind: if r.get::<_, String>(0)?.starts_with("web_") {
                    "website"
                } else {
                    "app"
                }
                .into(),
                app_id: r.get(0)?,
                name: r.get(1)?,
                enabled: r.get(2)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }
    pub fn target_enabled(&self, id: &str, enabled: bool) -> Result<()> {
        if self.connection.execute(
            "UPDATE launch_targets SET enabled=?2 WHERE app_id=?1",
            params![id, enabled],
        )? == 0
        {
            return Err(crate::rule("启动目标不存在"));
        }
        Ok(())
    }
    pub fn launch_path(&self, id: &str) -> Result<String> {
        self.connection
            .query_row(
                "SELECT path FROM launch_targets WHERE app_id=?1 AND enabled=1",
                [id],
                |r| r.get(0),
            )
            .optional()?
            .ok_or_else(|| crate::rule("应用启动许可已关闭"))
    }
    pub fn recommendation_feedback(&self, id: &str) -> Result<(f64, f64)> {
        Ok(self
            .connection
            .query_row(
                "SELECT alpha,beta FROM recommendation_feedback WHERE app_id=?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?
            .unwrap_or((2.0, 2.0)))
    }
    pub fn offer(&self, p: &AppRecommendation) -> Result<()> {
        self.connection.execute(
            "INSERT INTO offers VALUES(?1,?2,'offered',?3)",
            params![p.id, p.app_id, p.created],
        )?;
        Ok(())
    }
    pub fn last_offer(&self) -> Result<i64> {
        Ok(self
            .connection
            .query_row("SELECT COALESCE(MAX(at),0) FROM offers", [], |r| r.get(0))?)
    }
    pub fn offer_status(&self, id: &str, status: &str) -> Result<()> {
        self.connection.execute(
            "UPDATE offers SET status=?2 WHERE id=?1",
            params![id, status],
        )?;
        Ok(())
    }
    pub fn offer_feedback(&self, id: &str, app_id: &str, accepted: bool) -> Result<()> {
        let (mut a, mut b) = self.recommendation_feedback(app_id)?;
        if accepted {
            a += 0.1;
        } else {
            b += 0.1;
        }
        if a + b > 20.0 {
            let scale = 16.0 / (a + b - 4.0);
            a = 2.0 + (a - 2.0) * scale;
            b = 2.0 + (b - 2.0) * scale;
        }
        let tx = self.connection.unchecked_transaction()?;
        tx.execute("INSERT INTO recommendation_feedback VALUES(?1,?2,?3) ON CONFLICT(app_id) DO UPDATE SET alpha=excluded.alpha,beta=excluded.beta",params![app_id,a,b])?;
        tx.execute(
            "UPDATE offers SET status=?2 WHERE id=?1",
            params![id, if accepted { "opened" } else { "dismissed" }],
        )?;
        tx.commit()?;
        Ok(())
    }
}
