use habitos_core::{
    engine::Engine,
    extensions::*,
    model::*,
    platform::{DemoPlatform, Platform},
    startup,
    store::Store,
    usage_score,
};
const AT: i64 = 1_800_000_000;
fn setup() -> (Engine, DemoPlatform) {
    let mut p = DemoPlatform::default();
    p.current.hour = 12;
    let mut e = Engine::new(Store::memory().unwrap(), true).unwrap();
    e.observe(p.snapshot().unwrap(), AT).unwrap();
    (e, p)
}
fn target(e: &mut Engine, id: &str, days: i64, hour: u8, previous: &str) {
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
    for n in (1..=days).rev() {
        let mut c = e.current.clone().unwrap();
        c.app_id = id.into();
        c.hour = hour;
        c.local_day = Some(20260900 + n);
        c.recent = vec![previous.into()];
        e.store.record_usage(&c, AT - n * 86400).unwrap();
    }
}
#[test]
fn midday_usage_updates_overall_slowly_and_period_more_without_repeat_inflation() {
    let (e, _) = setup();
    let mut c = e.current.unwrap();
    e.store.record_usage(&c, AT).unwrap();
    let score = e.store.usage_scores(AT, 2).unwrap().remove(0);
    assert!((score.overall - 0.01).abs() < 1e-9);
    assert!((score.period - 0.04).abs() < 1e-9);
    for n in 1..1000 {
        e.store.record_usage(&c, AT + n).unwrap();
    }
    let score = e.store.usage_scores(AT + 1000, 2).unwrap().remove(0);
    assert_eq!(score.days, 1);
    assert_eq!(score.period_days, 1);
    assert!(score.overall <= 0.01 && score.period <= 0.04);
    c.hour = 20;
    e.store.record_usage(&c, AT + 1001).unwrap();
    let evening = e.store.usage_scores(AT + 1001, 4).unwrap().remove(0);
    assert_eq!(evening.days, 1);
    assert_eq!(evening.period_days, 1);
    c.hour = 12;
    c.local_day = Some(20261009);
    e.store.record_usage(&c, AT + 86400).unwrap();
    let score = e.store.usage_scores(AT + 86400, 2).unwrap().remove(0);
    assert_eq!(score.days, 2);
    assert_eq!(score.period_days, 2);
    assert!(score.overall > 0.01 && score.overall < 0.02);
    assert!(score.period > 0.04 && score.period < 0.08);
}
#[test]
fn returning_once_after_long_idle_does_not_resurrect_old_popularity() {
    let (mut e, _) = setup();
    target(&mut e, "old", 30, 12, "");
    let initial = e.store.usage_scores(AT, 2).unwrap()[0].period;
    let mut c = e.current.clone().unwrap();
    c.app_id = "old".into();
    c.local_day = Some(20271008);
    e.store.record_usage(&c, AT + 365 * 86400).unwrap();
    let after = e.store.usage_scores(AT + 365 * 86400, 2).unwrap()[0].period;
    assert!(initial > 0.1);
    assert!(after < 0.041);
}
#[test]
fn current_period_dominates_ranking_and_top_six_are_deterministic() {
    let (mut e, _) = setup();
    target(&mut e, "morning", 30, 8, "");
    target(&mut e, "midday", 5, 12, "");
    for n in 0..6 {
        target(&mut e, &format!("other{n}"), 1, 20, "");
    }
    let ranked = e.quick_apps(AT).unwrap();
    assert_eq!(ranked.len(), 6);
    assert_eq!(ranked[0].app_id, "midday");
    assert_eq!(
        ranked.iter().map(|a| &a.app_id).collect::<Vec<_>>(),
        e.quick_apps(AT)
            .unwrap()
            .iter()
            .map(|a| &a.app_id)
            .collect::<Vec<_>>()
    );
    e.target_enabled("midday", false).unwrap();
    assert!(e
        .quick_apps(AT)
        .unwrap()
        .iter()
        .all(|a| a.app_id != "midday"));
}
#[test]
fn associations_need_independent_days_are_bounded_and_can_be_disabled() {
    let (mut e, _) = setup();
    let current = e.current.as_ref().unwrap().app_id.clone();
    target(&mut e, "one", 1, 12, &current);
    target(&mut e, "regular", 6, 12, &current);
    let ranked = e.quick_apps(AT).unwrap();
    assert!(
        ranked
            .iter()
            .find(|a| a.app_id == "regular")
            .unwrap()
            .association_score
            > 0.0
    );
    assert_eq!(
        ranked
            .iter()
            .find(|a| a.app_id == "one")
            .unwrap()
            .association_score,
        0.0
    );
    for row in &ranked {
        assert!(row.score <= 0.20 * row.overall_score + 0.75 * row.period_score + 0.050000001);
    }
    let mut settings = e.settings.clone();
    settings.associations = false;
    e.save_settings(settings).unwrap();
    assert!(e
        .quick_apps(AT)
        .unwrap()
        .iter()
        .all(|a| a.association_score == 0.0));
}
#[test]
fn grid_launch_is_explicit_and_rechecks_context_permission_and_pause() {
    let (mut e, mut p) = setup();
    target(&mut e, "cold", 1, 12, "");
    assert!(e.app_recommendation.is_none());
    let session = e.current.as_ref().unwrap().session.clone();
    let rows = e.store.usage(AT).unwrap().len();
    assert!(e.open_target("cold", "stale", &mut p, AT + 1).is_err());
    e.open_target("cold", &session, &mut p, AT + 1).unwrap();
    assert_eq!(p.current.volume, 0.5);
    assert_eq!(e.store.usage(AT + 1).unwrap().len(), rows);
    e.permission("cold", false, Mode::Off).unwrap();
    assert!(e.open_target("cold", &session, &mut p, AT + 2).is_err());
    e.permission("cold", true, Mode::Ask).unwrap();
    p.demo_scene("sensitive").unwrap();
    assert!(e.open_target("cold", &session, &mut p, AT + 3).is_err());
    let mut settings = e.settings.clone();
    settings.paused = true;
    e.save_settings(settings).unwrap();
    assert!(e.quick_apps(AT).unwrap().is_empty());
    assert!(e.open_target("cold", &session, &mut p, AT + 4).is_err());
}
#[test]
fn deleting_memory_removes_rank_accumulators_but_keeps_launch_permission() {
    let (mut e, _) = setup();
    target(&mut e, "editor", 8, 12, "");
    assert!(e.store.usage_scores(AT, 2).unwrap()[0].period > 0.0);
    e.delete_memory("editor").unwrap();
    assert!(e.store.usage_scores(AT, 2).unwrap().is_empty());
    assert!(e.store.launch_path("editor").is_ok());
    let ranked = e.quick_apps(AT).unwrap();
    assert_eq!(ranked[0].score, 0.0);
}
#[test]
fn v3_migration_backfills_only_real_usage_dates_and_preserves_volume() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("legacy.sqlite3");
    let (mut e, _) = setup();
    let context = e.current.clone().unwrap();
    e.store = Store::open(&path).unwrap();
    e.store
        .save_permission(&AppPermission {
            id: context.app_id.clone(),
            name: context.app_name.clone(),
            observe: true,
            volume: Mode::Ask,
            sensitive: false,
            explicit: true,
        })
        .unwrap();
    e.store
        .sample(&Sample {
            context: context.clone(),
            volume: 0.35,
            at: AT,
            source: "explicit".into(),
        })
        .unwrap();
    target(&mut e, "editor", 8, 12, "");
    drop(e);
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch("DROP TABLE period_scores; DROP TABLE app_scores; PRAGMA user_version=3;")
        .unwrap();
    drop(db);
    let store = Store::open(&path).unwrap();
    assert_eq!(store.count_samples().unwrap(), 1);
    let score = store.usage_scores(AT, 2).unwrap().remove(0);
    assert_eq!(score.days, 8);
    assert_eq!(score.period_days, 8);
    assert!(score.period > score.overall);
    assert_eq!(store.usage(AT).unwrap().len(), 8);
}
#[test]
fn extension_contract_rejects_invalid_modules_and_bad_hints() {
    struct TestProvider {
        scope: String,
        score: f64,
    }
    impl SuggestionProvider for TestProvider {
        fn info(&self) -> ExtensionInfo {
            ExtensionInfo {
                id: "test".into(),
                name: "Test".into(),
                api_version: API_VERSION,
                scope: self.scope.clone(),
            }
        }
        fn suggest(&self, _: &Context, _: &[Usage], _: i64) -> Vec<RankHint> {
            vec![RankHint {
                app_id: "target".into(),
                score: self.score,
                independent_days: 8,
            }]
        }
    }
    let mut registry = Registry::standard();
    assert!(registry
        .register(Box::new(TestProvider {
            scope: "executor".into(),
            score: 1.0
        }))
        .is_err());
    registry
        .register(Box::new(TestProvider {
            scope: "usage_metadata".into(),
            score: f64::NAN,
        }))
        .unwrap();
    assert!(registry
        .register(Box::new(TestProvider {
            scope: "usage_metadata".into(),
            score: 0.5
        }))
        .is_err());
    let (e, _) = setup();
    assert!(registry.suggest(&e.current.unwrap(), &[], AT).is_empty());
    let caps = capabilities(true);
    assert!(caps.iter().find(|c| c.id == "volume").unwrap().available);
    assert!(
        caps.iter()
            .find(|c| c.id == "brightness")
            .unwrap()
            .available
    );
    assert!(!caps.iter().find(|c| c.id == "open_app").unwrap().autonomous);
}
#[test]
fn startup_quotes_paths_validates_length_and_demo_never_writes_real_startup() {
    assert_eq!(
        startup::command("C:\\Program Files\\satori\\satori.exe").unwrap(),
        "\"C:\\Program Files\\satori\\satori.exe\" --background"
    );
    assert!(startup::command("bad\"path").is_err());
    assert!(startup::command(&"a".repeat(261)).is_err());
    let mut p = DemoPlatform::default();
    assert!(!p.startup_status().unwrap().enabled);
    p.set_startup(true).unwrap();
    let state = p.startup_status().unwrap();
    assert!(state.enabled && state.simulated);
    p.set_startup(false).unwrap();
    assert!(!p.startup_status().unwrap().enabled);
    assert_eq!(usage_score::period(11), 2);
    assert_eq!(usage_score::period(13), 2);
    assert_eq!(usage_score::period(14), 3);
}
