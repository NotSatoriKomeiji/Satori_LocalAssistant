use habitos_core::{
    engine::Engine,
    memory,
    model::*,
    platform::{DemoPlatform, Platform, Snapshot},
    recommendation,
    store::Store,
};
const AT: i64 = 1_800_000_000;
fn setup() -> (Engine, DemoPlatform) {
    let mut p = DemoPlatform::default();
    let mut e = Engine::new(Store::memory().unwrap(), true).unwrap();
    e.observe(p.snapshot().unwrap(), AT).unwrap();
    (e, p)
}
fn train(e: &mut Engine, id: &str, count: i64, hour: u8, at: i64) {
    e.store
        .save_permission(&AppPermission {
            id: id.into(),
            name: format!("{id}.exe"),
            observe: true,
            volume: Mode::Ask,
            sensitive: false,
            explicit: false,
        })
        .unwrap();
    e.store.register_target(id, "demo:target.exe").unwrap();
    let original = e.current.clone().unwrap();
    for n in (1..=count).rev() {
        let mut c = original.clone();
        c.app_id = id.into();
        c.hour = hour;
        c.local_day = Some(20260000 + n);
        c.recent = vec![original.app_id.clone()];
        e.store.record_usage(&c, at - n * 7 * 86400).unwrap();
    }
}
#[test]
fn one_day_repetition_cannot_create_a_recommendation_or_inflate_memory() {
    let (e, _) = setup();
    let c = e.current.clone().unwrap();
    for n in 1..100 {
        e.store.record_usage(&c, AT + n).unwrap();
    }
    let m = e.store.memories(AT + 100).unwrap().remove(0);
    assert_eq!(m.days, 1);
    assert!(m.importance < 0.15);
    let rows: Vec<_> = (0..100)
        .map(|n| Usage {
            app_id: "candidate".into(),
            local_day: 20261008,
            hour: 20,
            weekday: 3,
            previous: c.app_id.clone(),
            at: AT - n - 1,
        })
        .collect();
    assert!(recommendation::predict(&c, &rows, "candidate", "Candidate", AT, (2., 2.)).is_none());
}
#[test]
fn time_weekday_and_recent_context_rank_independent_days_conservatively() {
    let (mut e, _) = setup();
    train(&mut e, "editor", 8, 20, AT);
    let c = e.current.clone().unwrap();
    let rows = e.store.usage(AT).unwrap();
    let p = recommendation::predict(&c, &rows, "editor", "Editor", AT, (2., 2.)).unwrap();
    assert!(p.confidence >= 0.75 && p.confidence < 0.9);
    assert_eq!(p.days, 8);
    let mut wrong = c.clone();
    wrong.hour = 8;
    assert!(recommendation::predict(&wrong, &rows, "editor", "Editor", AT, (2., 2.)).is_none());
    wrong = c.clone();
    wrong.weekday = Some(6);
    let weekend = recommendation::predict(&wrong, &rows, "editor", "Editor", AT, (2., 2.)).unwrap();
    assert!(weekend.confidence < 0.75);
    wrong = c.clone();
    wrong.app_id = "other".into();
    wrong.recent.clear();
    assert!(
        recommendation::predict(&wrong, &rows, "editor", "Editor", AT, (2., 2.))
            .unwrap()
            .confidence
            < p.confidence
    );
}
#[test]
fn old_patterns_decay_and_missing_weekdays_abstain() {
    let (mut e, _) = setup();
    train(&mut e, "editor", 8, 20, AT - 30 * 86400);
    let c = e.current.clone().unwrap();
    let rows = e.store.usage(AT).unwrap();
    assert!(
        recommendation::predict(&c, &rows, "editor", "Editor", AT, (2., 2.))
            .is_none_or(|p| p.confidence < 0.75)
    );
    let mut old = c;
    old.weekday = None;
    assert!(recommendation::predict(&old, &rows, "editor", "Editor", AT, (2., 2.)).is_none());
}
#[test]
fn tied_candidates_abstain_and_recommendations_do_not_change_volume() {
    let (mut e, mut p) = setup();
    train(&mut e, "editor", 8, 20, AT);
    train(&mut e, "browser", 8, 20, AT);
    e.observe(p.snapshot().unwrap(), AT + 6).unwrap();
    assert!(e.app_recommendation.is_none());
    assert_eq!(p.current.volume, 0.5);
    e.target_enabled("browser", false).unwrap();
    e.request_decision();
    e.observe(p.snapshot().unwrap(), AT + 7).unwrap();
    assert!(e.app_recommendation.is_some());
    assert!(e.proposal.is_none());
    // A runner-up below the display threshold must still make the winner uncertain.
    let (mut other, mut platform) = setup();
    train(&mut other, "editor", 8, 20, AT);
    train(&mut other, "browser", 8, 21, AT);
    other.observe(platform.snapshot().unwrap(), AT + 6).unwrap();
    assert!(other.app_recommendation.is_none());
}
#[test]
fn cold_start_pause_sensitive_and_revocation_block_recommendation() {
    let (mut e, mut p) = setup();
    train(&mut e, "editor", 1, 20, AT);
    e.observe(p.snapshot().unwrap(), AT + 6).unwrap();
    assert!(e.app_recommendation.is_none());
    train(&mut e, "editor", 8, 20, AT);
    e.sensitive("editor", true).unwrap();
    e.request_decision();
    e.observe(p.snapshot().unwrap(), AT + 7).unwrap();
    assert!(e.app_recommendation.is_none());
    e.sensitive("editor", false).unwrap();
    e.permission("editor", true, Mode::Ask).unwrap();
    e.request_decision();
    e.observe(p.snapshot().unwrap(), AT + 8).unwrap();
    let id = e.app_recommendation.as_ref().unwrap().id.clone();
    e.target_enabled("editor", false).unwrap();
    assert!(e.launch_app(&id, &mut p, AT + 9).is_err());
    let mut settings = e.settings.clone();
    settings.paused = true;
    e.save_settings(settings).unwrap();
    e.request_decision();
    e.observe(p.snapshot().unwrap(), AT + 10).unwrap();
    assert!(e.app_recommendation.is_none());
}
#[test]
fn launch_is_explicit_audited_and_cooldown_survives_restart() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("database.sqlite3");
    let mut p = DemoPlatform::default();
    let mut e = Engine::new(Store::open(&path).unwrap(), true).unwrap();
    e.observe(p.snapshot().unwrap(), AT).unwrap();
    train(&mut e, "editor", 8, 20, AT);
    e.observe(p.snapshot().unwrap(), AT + 6).unwrap();
    let id = e.app_recommendation.as_ref().unwrap().id.clone();
    assert_eq!(e.store.recommendation_feedback("editor").unwrap(), (2., 2.));
    e.launch_app(&id, &mut p, AT + 7).unwrap();
    assert_eq!(
        e.store.recommendation_feedback("editor").unwrap(),
        (2.1, 2.)
    );
    assert!(e.app_recommendation.is_none());
    assert_eq!(p.current.volume, 0.5);
    drop(e);
    let mut e = Engine::new(Store::open(&path).unwrap(), true).unwrap();
    e.observe(p.snapshot().unwrap(), AT + 10).unwrap();
    e.observe(p.snapshot().unwrap(), AT + 16).unwrap();
    assert!(e.app_recommendation.is_none());
}
#[test]
fn stale_context_and_expired_cards_cannot_launch() {
    let (mut e, mut p) = setup();
    train(&mut e, "editor", 8, 20, AT);
    e.observe(p.snapshot().unwrap(), AT + 6).unwrap();
    let id = e.app_recommendation.as_ref().unwrap().id.clone();
    assert!(e.launch_app(&id, &mut p, AT + 66).is_err());
    p.demo_scene("sensitive").unwrap();
    assert!(e.launch_app(&id, &mut p, AT + 7).is_err());
    assert_eq!(e.store.recommendation_feedback("editor").unwrap(), (2., 2.));
}
#[test]
fn failed_launch_gets_no_positive_feedback() {
    struct Failed(Snapshot);
    impl Platform for Failed {
        fn snapshot(&mut self) -> habitos_core::Result<Snapshot> {
            Ok(self.0.clone())
        }
        fn set_volume(&mut self, _: &Snapshot, _: f64) -> habitos_core::Result<()> {
            Ok(())
        }
        fn launch_program(&mut self, _: &str) -> habitos_core::Result<()> {
            Err(habitos_core::Error::Rule("missing file".into()))
        }
    }
    let (mut e, mut p) = setup();
    train(&mut e, "editor", 8, 20, AT);
    e.observe(p.snapshot().unwrap(), AT + 6).unwrap();
    let id = e.app_recommendation.as_ref().unwrap().id.clone();
    let mut failing = Failed(p.snapshot().unwrap());
    assert!(e.launch_app(&id, &mut failing, AT + 7).is_err());
    assert_eq!(e.store.recommendation_feedback("editor").unwrap(), (2., 2.));
    assert!(e.app_recommendation.is_none());
}
#[test]
fn importance_decay_pin_and_forgetting_are_independent_of_permissions() {
    assert!((memory::importance(0.8, AT, AT + 30 * 86400, false) - 0.4).abs() < 1e-9);
    let (mut e, mut p) = setup();
    e.observe(p.snapshot().unwrap(), AT + 6).unwrap();
    e.capture(AT + 6, "explicit").unwrap();
    let app = e.current.as_ref().unwrap().app_id.clone();
    e.pin_memory(&app, true).unwrap();
    e.store.prune(AT + 365 * 86400, 90).unwrap();
    assert_eq!(e.store.count_samples().unwrap(), 1);
    assert_eq!(
        e.store.memories(AT + 365 * 86400).unwrap()[0].importance,
        1.
    );
    e.delete_memory(&app).unwrap();
    assert!(e.store.memories(AT + 365 * 86400).unwrap().is_empty());
    assert_eq!(e.store.count_samples().unwrap(), 0);
    assert!(e.store.permission(&app).unwrap().unwrap().observe);
    e.observe(p.snapshot().unwrap(), AT + 7).unwrap();
    assert!(e.store.memories(AT + 7).unwrap().is_empty());
    e.request_decision();
    e.observe(p.snapshot().unwrap(), AT + 400 * 86400).unwrap();
    let memories = e.store.memories(AT + 400 * 86400).unwrap();
    assert_eq!(memories.len(), 1);
    assert_eq!(memories[0].days, 1);
    assert_eq!(memories[0].samples, 0);
}
#[test]
fn unused_low_importance_memory_is_removed_and_feedback_stays_bounded() {
    let (mut e, mut p) = setup();
    e.observe(p.snapshot().unwrap(), AT + 6).unwrap();
    assert_eq!(e.store.memories(AT + 6).unwrap().len(), 1);
    e.store.prune(AT + 91 * 86400, 90).unwrap();
    assert!(e.store.memories(AT + 91 * 86400).unwrap().is_empty());
    train(&mut e, "editor", 8, 20, AT);
    let offer = AppRecommendation {
        id: "test".into(),
        app_id: "editor".into(),
        app_name: "Editor".into(),
        confidence: 0.8,
        days: 8,
        reason: String::new(),
        created: AT,
    };
    e.store.offer(&offer).unwrap();
    for _ in 0..500 {
        e.store.offer_feedback("test", "editor", false).unwrap();
    }
    let (a, b) = e.store.recommendation_feedback("editor").unwrap();
    assert!(a + b <= 20.01);
}
#[test]
fn v2_migration_preserves_volume_samples_and_adds_legacy_memory_without_fake_usage() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("legacy.sqlite3");
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch("CREATE TABLE apps(id TEXT PRIMARY KEY,name TEXT NOT NULL,observe INTEGER NOT NULL,mode TEXT NOT NULL,sensitive INTEGER NOT NULL,explicit INTEGER NOT NULL); INSERT INTO apps VALUES('old','Old.exe',1,'\"ask\"',0,1); CREATE TABLE behavior_events(id INTEGER PRIMARY KEY,app_id TEXT NOT NULL,app_name TEXT NOT NULL,kind TEXT NOT NULL,at INTEGER NOT NULL); CREATE TABLE samples(app_id TEXT NOT NULL,device TEXT NOT NULL,session TEXT NOT NULL,context TEXT NOT NULL,volume REAL NOT NULL,at INTEGER NOT NULL,source TEXT NOT NULL,PRIMARY KEY(app_id,device,session)); PRAGMA user_version=2;").unwrap();
    let (e, _) = setup();
    let mut json = serde_json::to_value(e.current.unwrap()).unwrap();
    json["app_id"] = "old".into();
    json["device"] = "device".into();
    json.as_object_mut().unwrap().remove("weekday");
    json.as_object_mut().unwrap().remove("local_day");
    db.execute(
        "INSERT INTO samples VALUES('old','device','legacy',?1,0.35,?2,'explicit')",
        rusqlite::params![json.to_string(), AT],
    )
    .unwrap();
    drop(db);
    let store = Store::open(&path).unwrap();
    assert_eq!(store.count_samples().unwrap(), 1);
    assert_eq!(store.memories(AT).unwrap()[0].name, "Old.exe");
    assert!(store.usage(AT).unwrap().is_empty());
    assert!(store.settings().unwrap().time_recommendations);
    let c: Context = serde_json::from_value(json).unwrap();
    assert!(c.weekday.is_none());
    assert_eq!(store.samples(&c, AT, 90).unwrap().len(), 1);
}
#[test]
fn demo_fixture_is_isolated_from_real_mode() {
    let (mut e, _) = setup();
    e.demo_recommend(AT).unwrap();
    assert!(e.app_recommendation.is_some());
    assert_eq!(e.store.memories(AT).unwrap()[0].days, 8);
    let mut real = Engine::new(Store::memory().unwrap(), false).unwrap();
    let mut p = DemoPlatform::default();
    real.observe(p.snapshot().unwrap(), AT).unwrap();
    assert!(real.demo_recommend(AT).is_err());
}

#[test]
fn volume_apply_and_undo_coexist_with_app_cards_without_cross_training() {
    let (mut e, mut p) = setup();
    e.grant_current().unwrap();
    for n in 1..=5 {
        let mut context = e.current.clone().unwrap();
        context.session = format!("volume-history-{n}");
        e.store
            .sample(&Sample {
                context,
                volume: 0.35,
                at: AT - n * 3600,
                source: "explicit".into(),
            })
            .unwrap();
    }
    train(&mut e, "editor", 8, 20, AT);
    e.observe(p.snapshot().unwrap(), AT + 6).unwrap();
    let app_id = e.app_recommendation.as_ref().unwrap().id.clone();
    let volume_id = e.proposal.as_ref().unwrap().id.clone();
    let usage = e.store.usage(AT + 6).unwrap().len();
    e.apply(&volume_id, false, &mut p, AT + 7).unwrap();
    assert_eq!(e.app_recommendation.as_ref().unwrap().id, app_id);
    let action = e.store.journal().unwrap().remove(0);
    e.undo(&action.id, &mut p, AT + 8).unwrap();
    assert!((p.current.volume - 0.5).abs() < 1e-9);
    e.launch_app(&app_id, &mut p, AT + 9).unwrap();
    assert_eq!(e.store.count_samples().unwrap(), 5);
    assert_eq!(e.store.usage(AT + 9).unwrap().len(), usage);
    assert_eq!(
        e.store.recommendation_feedback("editor").unwrap(),
        (2.1, 2.)
    );
}

#[test]
fn target_registration_validates_files_and_never_revives_sensitive_rules() {
    let (mut e, _) = setup();
    let dir = tempfile::tempdir().unwrap();
    let exe = dir.path().join("Ordinary.exe");
    std::fs::write(&exe, b"fixture").unwrap();
    assert!(e.register_target("relative.exe").is_err());
    assert!(e.register_target(dir.path().to_str().unwrap()).is_err());
    e.register_target(exe.to_str().unwrap()).unwrap();
    let id = habitos_core::platform::identity(exe.to_str().unwrap());
    let state = serde_json::to_string(&e.status(AT).unwrap()).unwrap();
    assert!(!state.contains(exe.to_str().unwrap()));
    e.sensitive(&id, true).unwrap();
    assert!(e.register_target(exe.to_str().unwrap()).is_err());
    assert!(e.store.permission(&id).unwrap().unwrap().sensitive);
}

#[test]
fn chronological_holdout_preflight_rejects_bursts_outliers_and_future_leakage() {
    let (e, _) = setup();
    let c = e.current.unwrap();
    let dates = [
        20260813, 20260820, 20260827, 20260903, 20260910, 20260917, 20260924, 20261001,
    ];
    let history: Vec<Usage> = dates
        .iter()
        .enumerate()
        .map(|(i, day)| Usage {
            app_id: "editor".into(),
            local_day: *day,
            hour: 20,
            weekday: 3,
            previous: c.app_id.clone(),
            at: AT - (8 - i as i64) * 7 * 86400,
        })
        .collect();
    let score = |context: &Context, rows: &[Usage]| {
        recommendation::predict(context, rows, "editor", "Editor", AT, (2., 2.))
            .map(|p| p.confidence)
    };
    let regular = score(&c, &history).unwrap();
    assert!((0.75..0.85).contains(&regular));
    let mut weekend = c.clone();
    weekend.weekday = Some(6);
    let weekend_score = score(&weekend, &history).unwrap();
    assert!(weekend_score < 0.75);
    let mut morning = c.clone();
    morning.hour = 8;
    assert!(score(&morning, &history).is_none());
    let burst = vec![history[7].clone(); 1000];
    assert!(score(&c, &burst).is_none());
    assert!(score(&c, &history[4..]).is_none());
    let mut contaminated = history.clone();
    let mut outlier = history[7].clone();
    outlier.local_day = 20261007;
    outlier.hour = 8;
    outlier.at = AT - 86400;
    contaminated.push(outlier);
    assert!((score(&c, &contaminated).unwrap() - regular).abs() < 1e-9);
    assert!(score(&morning, &contaminated).is_none());
    for n in 1..=100 {
        let mut future = history[7].clone();
        future.local_day = 20270000 + n;
        future.at = AT + n;
        contaminated.push(future);
    }
    assert!((score(&c, &contaminated).unwrap() - regular).abs() < 1e-9);
    let old: Vec<_> = history
        .iter()
        .cloned()
        .map(|mut row| {
            row.at -= 30 * 86400;
            row
        })
        .collect();
    let stale = score(&c, &old);
    assert!(stale.is_none_or(|value| value < 0.75));
    println!("PREFLIGHT regular={regular:.6} weekend={weekend_score:.6} stale={stale:?}; burst/cold/morning=abstain; outlier/future=no influence");
}

#[test]
fn pinned_memory_survives_retention_with_bounded_raw_samples() {
    let (e, _) = setup();
    let c = e.current.unwrap();
    for n in 1..=450 {
        let mut context = c.clone();
        context.session = format!("bounded-{n}");
        context.local_day = Some(20000000 + n);
        e.store
            .sample(&Sample {
                context: context.clone(),
                volume: 0.35,
                at: AT - n,
                source: "explicit".into(),
            })
            .unwrap();
        e.store.record_usage(&context, AT - n).unwrap();
    }
    e.store.pin(&c.app_id, true).unwrap();
    e.store.prune(AT + 365 * 86400, 90).unwrap();
    assert_eq!(e.store.count_samples().unwrap(), 200);
    assert_eq!(e.store.usage(AT).unwrap().len(), 400);
    let m = e.store.memories(AT + 365 * 86400).unwrap().remove(0);
    assert!(m.pinned);
    assert_eq!(m.importance, 1.);
    assert!(recommendation::predict(
        &c,
        &e.store.usage(AT + 365 * 86400).unwrap(),
        &c.app_id,
        &c.app_name,
        AT + 365 * 86400,
        (2., 2.)
    )
    .is_none());
}
