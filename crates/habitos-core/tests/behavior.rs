use habitos_core::{
    engine::Engine,
    learning,
    model::*,
    platform::{DemoPlatform, Platform, Snapshot},
    store::Store,
};

const AT: i64 = 1_800_000_000;
fn setup(demo: bool) -> (Engine, DemoPlatform) {
    let mut platform = DemoPlatform::default();
    let mut engine = Engine::new(Store::memory().unwrap(), demo).unwrap();
    engine.observe(platform.snapshot().unwrap(), AT).unwrap();
    (engine, platform)
}
fn train(engine: &mut Engine, count: usize, value: f64) {
    engine.grant_current().unwrap();
    for i in 0..count {
        let mut context = engine.current.clone().unwrap();
        context.session = format!("past-{i}");
        engine
            .store
            .sample(&Sample {
                context,
                volume: value,
                at: AT - (i as i64 + 1) * 3600,
                source: "explicit".into(),
            })
            .unwrap();
    }
}
fn suggestion(engine: &mut Engine, platform: &mut DemoPlatform) -> String {
    engine
        .observe(platform.snapshot().unwrap(), AT + 6)
        .unwrap();
    engine.proposal.as_ref().expect("suggestion").id.clone()
}

#[test]
fn ordinary_apps_record_and_learn_automatically_without_a_grant() {
    let (mut e, mut p) = setup(false);
    assert_eq!(e.store.event_count().unwrap(), 1);
    let permission = e.store.permission(&p.current.app_id).unwrap().unwrap();
    assert!(permission.observe);
    assert!(!permission.explicit);
    assert_eq!(permission.volume, Mode::Auto);
    p.current.volume = 0.30;
    e.observe(p.snapshot().unwrap(), AT + 6).unwrap();
    e.observe(p.snapshot().unwrap(), AT + 12).unwrap();
    assert_eq!(e.store.count_samples().unwrap(), 1);
    assert!(e.proposal.is_none());
}
#[test]
fn repeated_capture_does_not_inflate_independent_evidence() {
    let (mut e, _) = setup(false);
    e.grant_current().unwrap();
    e.capture(AT + 6, "explicit").unwrap();
    e.capture(AT + 7, "explicit").unwrap();
    assert_eq!(e.store.count_samples().unwrap(), 1);
}
#[test]
fn external_change_settles_and_is_recorded_once() {
    let (mut e, mut p) = setup(false);
    e.grant_current().unwrap();
    p.current.volume = 0.30;
    e.observe(p.snapshot().unwrap(), AT + 6).unwrap();
    p.current.volume = 0.35;
    e.observe(p.snapshot().unwrap(), AT + 7).unwrap();
    assert_eq!(e.store.count_samples().unwrap(), 0);
    e.observe(p.snapshot().unwrap(), AT + 11).unwrap();
    assert_eq!(e.store.count_samples().unwrap(), 1);
}
#[test]
fn cold_start_and_device_mismatch_abstain() {
    let (mut e, mut p) = setup(false);
    train(&mut e, 2, 0.35);
    e.observe(p.snapshot().unwrap(), AT + 6).unwrap();
    assert!(e.proposal.is_none());
    train(&mut e, 5, 0.35);
    p.current.device = "different-device".into();
    e.observe(p.snapshot().unwrap(), AT + 12).unwrap();
    e.observe(p.snapshot().unwrap(), AT + 18).unwrap();
    assert!(e.proposal.is_none());
}
#[test]
fn explicit_execution_is_verified_journaled_and_not_retrained() {
    let (mut e, mut p) = setup(false);
    train(&mut e, 5, 0.35);
    let id = suggestion(&mut e, &mut p);
    assert!(!e.auto_eligible().unwrap());
    e.apply(&id, false, &mut p, AT + 7).unwrap();
    assert!((p.current.volume - 0.35).abs() < 0.01);
    assert_eq!(e.store.count_samples().unwrap(), 5);
    e.observe(p.snapshot().unwrap(), AT + 12).unwrap();
    assert_eq!(e.store.count_samples().unwrap(), 5);
    let row = e.store.journal().unwrap().remove(0);
    assert_eq!(row.status, "applied");
    e.undo(&row.id, &mut p, AT + 13).unwrap();
    assert!((p.current.volume - 0.50).abs() < 0.01);
    assert_eq!(e.store.journal().unwrap()[0].status, "undone");
}
#[test]
fn automatic_execution_requires_grant_and_does_not_add_feedback() {
    let (mut e, mut p) = setup(false);
    train(&mut e, 5, 0.35);
    let id = suggestion(&mut e, &mut p);
    assert!(e.apply(&id, true, &mut p, AT + 7).is_err());
    let app = e.current.as_ref().unwrap().app_id.clone();
    e.permission(&app, true, Mode::Auto).unwrap();
    let id = suggestion(&mut e, &mut p);
    assert!(e.auto_eligible().unwrap());
    e.apply(&id, true, &mut p, AT + 7).unwrap();
    assert_eq!(
        e.store.feedback(e.current.as_ref().unwrap()).unwrap(),
        (1.0, 1.0)
    );
    assert_eq!(e.store.count_samples().unwrap(), 5);
}
#[test]
fn revoked_permissions_and_pause_cancel_pending_actions() {
    let (mut e, mut p) = setup(false);
    train(&mut e, 5, 0.35);
    let id = suggestion(&mut e, &mut p);
    let app = e.current.as_ref().unwrap().app_id.clone();
    e.permission(&app, false, Mode::Off).unwrap();
    assert!(e.apply(&id, false, &mut p, AT + 7).is_err());
    assert_eq!(p.current.volume, 0.5);
    e.permission(&app, true, Mode::Ask).unwrap();
    let id = suggestion(&mut e, &mut p);
    let mut settings = e.settings.clone();
    settings.paused = true;
    e.save_settings(settings).unwrap();
    assert!(e.apply(&id, false, &mut p, AT + 7).is_err());
    assert!(e.capture(AT + 7, "explicit").is_err());
}
#[test]
fn fresh_native_context_is_checked_even_before_observer_refreshes() {
    let (mut e, mut p) = setup(false);
    train(&mut e, 5, 0.35);
    let id = suggestion(&mut e, &mut p);
    p.demo_scene("editor").unwrap();
    assert!(e.apply(&id, false, &mut p, AT + 7).is_err());
    assert_eq!(p.current.volume, 0.5);
    assert!(e.current.is_none());
}
#[test]
fn external_volume_change_cancels_execution_and_undo() {
    let (mut e, mut p) = setup(false);
    train(&mut e, 5, 0.35);
    let id = suggestion(&mut e, &mut p);
    p.current.volume = 0.6;
    assert!(e.apply(&id, false, &mut p, AT + 7).is_err());
    p.current.volume = 0.5;
    e.observe(p.snapshot().unwrap(), AT + 70).unwrap();
    e.request_decision();
    e.observe(p.snapshot().unwrap(), AT + 131).unwrap();
    assert!(
        e.proposal.is_none(),
        "manual changes hold this session even after cooldown"
    );
    p.demo_scene("editor").unwrap();
    e.observe(p.snapshot().unwrap(), AT + 132).unwrap();
    p.demo_scene("game").unwrap();
    e.observe(p.snapshot().unwrap(), AT + 133).unwrap();
    e.observe(p.snapshot().unwrap(), AT + 140).unwrap();
    let id = e.proposal.as_ref().unwrap().id.clone();
    e.apply(&id, false, &mut p, AT + 141).unwrap();
    let row = e.store.journal().unwrap().remove(0);
    p.current.volume = 0.7;
    assert!(e.undo(&row.id, &mut p, AT + 142).is_err());
    assert_eq!(p.current.volume, 0.7);
}
#[test]
fn proposal_expiry_rejection_cooldown_and_bounded_feedback() {
    let (mut e, mut p) = setup(false);
    train(&mut e, 5, 0.35);
    let id = suggestion(&mut e, &mut p);
    assert!(e.apply(&id, false, &mut p, AT + 37).is_err());
    e.observe(p.snapshot().unwrap(), AT + 37).unwrap();
    assert!(e.proposal.is_none());
    e.observe(p.snapshot().unwrap(), AT + 50).unwrap();
    assert!(e.proposal.is_none());
    e.request_decision();
    e.observe(p.snapshot().unwrap(), AT + 100).unwrap();
    let id = e.proposal.as_ref().unwrap().id.clone();
    e.reject(&id, AT + 101).unwrap();
    assert!(e.proposal.is_none());
    e.observe(p.snapshot().unwrap(), AT + 110).unwrap();
    assert!(e.proposal.is_none());
    for _ in 0..500 {
        e.store
            .record_feedback(e.current.as_ref().unwrap(), false)
            .unwrap();
    }
    let (a, b) = e.store.feedback(e.current.as_ref().unwrap()).unwrap();
    assert!(a + b <= 50.01);
}
#[test]
fn similarity_renormalizes_missing_features_and_time_wraps() {
    let (e, _) = setup(false);
    let a = e.current.unwrap();
    let mut b = a.clone();
    assert!((learning::similarity(&a, &b) - 1.0).abs() < 1e-12);
    b.hour = 21;
    let one_hour = learning::similarity(&a, &b);
    let mut x = a.clone();
    x.hour = 23;
    b.hour = 0;
    assert!((learning::similarity(&x, &b) - one_hour).abs() < 1e-12);
}
#[test]
fn outlier_filter_retains_robust_preference_and_low_confidence_abstains() {
    let (mut e, _) = setup(false);
    train(&mut e, 5, 0.35);
    let c = e.current.clone().unwrap();
    let mut rows = e.store.samples(&c, AT, 90).unwrap();
    let mut outlier = rows[0].clone();
    outlier.volume = 0.95;
    outlier.context.session = "outlier".into();
    rows.push(outlier);
    let prediction = learning::predict(&c, &rows, 1.0, 1.0, AT).unwrap();
    assert!((prediction.target - 0.35).abs() < 1e-9);
    assert_eq!(prediction.samples, 5);
    for (i, row) in rows.iter_mut().enumerate() {
        row.volume = if i % 2 == 0 { 0.25 } else { 0.50 };
    }
    let p = learning::predict(&c, &rows, 1.0, 1.0, AT);
    assert!(p.is_none_or(|p| p.confidence < 0.80));
}
#[test]
fn settings_and_demo_injection_are_validated_at_backend() {
    let (mut e, _) = setup(false);
    e.grant_current().unwrap();
    assert!(e.demo_teach(AT).is_err());
    let mut settings = e.settings.clone();
    settings.auto_threshold = 0.1;
    assert!(e.save_settings(settings).is_err());
    let mut settings = e.settings.clone();
    settings.max_volume = f64::NAN;
    assert!(e.save_settings(settings).is_err());
    let (mut demo, _) = setup(true);
    demo.grant_current().unwrap();
    demo.demo_teach(AT).unwrap();
    assert_eq!(demo.store.count_samples().unwrap(), 5);
    assert!(demo.proposal.is_some());
}
#[test]
fn database_persistence_pruning_forgetting_and_interrupted_journal() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.db");
    {
        let mut e = Engine::new(Store::open(&path).unwrap(), false).unwrap();
        let mut p = DemoPlatform::default();
        e.observe(p.snapshot().unwrap(), AT).unwrap();
        train(&mut e, 5, 0.35);
        let c = e.current.as_ref().unwrap();
        e.store
            .prepare_action(
                &JournalEntry {
                    id: "interrupted".into(),
                    app_name: c.app_name.clone(),
                    device: c.device.clone(),
                    before: 0.5,
                    after: 0.35,
                    status: "prepared".into(),
                    at: AT,
                    undoable: false,
                },
                &c.app_id,
            )
            .unwrap();
    }
    let mut store = Store::open(&path).unwrap();
    assert_eq!(store.count_samples().unwrap(), 5);
    assert_eq!(store.journal().unwrap()[0].status, "interrupted");
    assert_eq!(store.permissions().unwrap().len(), 1);
    store.prune(AT + 91 * 86400, 90).unwrap();
    assert_eq!(store.count_samples().unwrap(), 0);
    store.forget().unwrap();
    assert!(store.journal().unwrap().is_empty());
    assert_eq!(store.permissions().unwrap().len(), 1);
}
#[test]
fn volume_ceiling_blocks_suggestions_and_auto_jump_limit_is_independent() {
    let (mut e, mut p) = setup(false);
    train(&mut e, 5, 0.95);
    e.observe(p.snapshot().unwrap(), AT + 6).unwrap();
    assert!(e.proposal.is_none());
    let (mut e, mut p) = setup(false);
    train(&mut e, 5, 0.2);
    let app = e.current.as_ref().unwrap().app_id.clone();
    e.permission(&app, true, Mode::Auto).unwrap();
    suggestion(&mut e, &mut p);
    assert!(e.auto_eligible().unwrap());
    assert!((e.proposal.as_ref().unwrap().prediction.target - p.current.volume).abs() <= 0.050001);
    let mut settings = e.settings.clone();
    settings.auto_adjustments = false;
    e.save_settings(settings).unwrap();
    assert!(!e.auto_eligible().unwrap());
}
#[test]
fn platform_failure_never_marks_action_as_applied() {
    struct Failed {
        snapshot: Snapshot,
    }
    impl Platform for Failed {
        fn snapshot(&mut self) -> habitos_core::Result<Snapshot> {
            Ok(self.snapshot.clone())
        }
        fn set_volume(&mut self, _: &Snapshot, _: f64) -> habitos_core::Result<()> {
            Err(habitos_core::Error::Rule("device unavailable".into()))
        }
    }
    let (mut e, mut p) = setup(false);
    train(&mut e, 5, 0.35);
    let id = suggestion(&mut e, &mut p);
    let mut failing = Failed {
        snapshot: p.snapshot().unwrap(),
    };
    assert!(e.apply(&id, false, &mut failing, AT + 7).is_err());
    assert_eq!(e.store.journal().unwrap()[0].status, "failed");
}

#[test]
fn sensitive_apps_require_permission_and_never_leak_into_recent_history() {
    let (mut e, mut p) = setup(false);
    p.demo_scene("sensitive").unwrap();
    e.observe(p.snapshot().unwrap(), AT + 10).unwrap();
    let id = p.current.app_id.clone();
    let permission = e.store.permission(&id).unwrap().unwrap();
    assert!(permission.sensitive && !permission.observe && !permission.explicit);
    e.log_signal("time_node", AT + 11).unwrap();
    p.current.volume = 0.3;
    e.observe(p.snapshot().unwrap(), AT + 12).unwrap();
    e.observe(p.snapshot().unwrap(), AT + 20).unwrap();
    assert_eq!(e.store.event_count().unwrap(), 1);
    assert!(e.capture(AT + 20, "explicit").is_err());
    assert_eq!(e.store.count_samples().unwrap(), 0);
    p.demo_scene("editor").unwrap();
    e.observe(p.snapshot().unwrap(), AT + 21).unwrap();
    assert!(!e.current.as_ref().unwrap().recent.contains(&id));
    p.demo_scene("sensitive").unwrap();
    e.observe(p.snapshot().unwrap(), AT + 22).unwrap();
    e.grant_current().unwrap();
    e.log_signal("time_node", AT + 23).unwrap();
    e.capture(AT + 28, "explicit").unwrap();
    assert_eq!(e.store.count_samples().unwrap(), 1);
    assert_eq!(e.store.events().unwrap()[0].app_name, "KeePass.exe");
}

#[test]
fn custom_sensitive_marks_and_opt_out_survive_scene_changes() {
    let (mut e, mut p) = setup(false);
    let id = p.current.app_id.clone();
    e.sensitive(&id, true).unwrap();
    e.observe(p.snapshot().unwrap(), AT + 6).unwrap();
    assert!(e.capture(AT + 7, "explicit").is_err());
    assert!(e.next_deadline().is_none());
    e.permission(&id, false, Mode::Off).unwrap();
    p.demo_scene("editor").unwrap();
    e.observe(p.snapshot().unwrap(), AT + 10).unwrap();
    p.demo_scene("game").unwrap();
    e.observe(p.snapshot().unwrap(), AT + 20).unwrap();
    let rule = e.store.permission(&id).unwrap().unwrap();
    assert!(rule.sensitive && rule.explicit && !rule.observe);
    assert_eq!(e.store.event_count().unwrap(), 2);
}

#[test]
fn desktop_auto_events_work_without_audio_and_without_fake_volume_samples() {
    let (mut e, mut p) = setup(false);
    p.demo_scene("desktop").unwrap();
    p.current.audio_available = false;
    p.current.device = "no-audio".into();
    e.observe(p.snapshot().unwrap(), AT + 10).unwrap();
    e.observe(p.snapshot().unwrap(), AT + 16).unwrap();
    assert_eq!(e.store.events().unwrap()[0].kind, "desktop_focus");
    assert!(e.volume.is_none());
    assert!(e.capture(AT + 16, "explicit").is_err());
    assert_eq!(e.store.count_samples().unwrap(), 0);
    assert!(e.proposal.is_none());
    assert!(e.next_deadline().is_none());
}

#[test]
fn global_auto_learning_switch_preserves_explicit_permissions() {
    let (mut e, mut p) = setup(false);
    let mut settings = e.settings.clone();
    settings.automatic_learning = false;
    e.save_settings(settings).unwrap();
    e.log_signal("time_node", AT + 1).unwrap();
    assert_eq!(e.store.event_count().unwrap(), 1);
    assert!(e.capture(AT + 6, "explicit").is_err());
    e.grant_current().unwrap();
    e.capture(AT + 7, "explicit").unwrap();
    p.demo_scene("editor").unwrap();
    e.observe(p.snapshot().unwrap(), AT + 10).unwrap();
    assert!(
        !e.store
            .permission(&p.current.app_id)
            .unwrap()
            .unwrap()
            .observe
    );
    assert_eq!(e.store.event_count().unwrap(), 1);
    let mut settings = e.settings.clone();
    settings.automatic_learning = true;
    e.save_settings(settings).unwrap();
    e.log_signal("time_node", AT + 11).unwrap();
    assert_eq!(e.store.event_count().unwrap(), 2);
    assert!(
        e.store
            .permission(&p.current.app_id)
            .unwrap()
            .unwrap()
            .observe
    );
}

#[test]
fn v1_database_migration_preserves_grants_denials_and_legacy_settings() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("legacy.sqlite3");
    let old = rusqlite::Connection::open(&path).unwrap();
    old.execute_batch("CREATE TABLE apps(id TEXT PRIMARY KEY,name TEXT NOT NULL,observe INTEGER NOT NULL,mode TEXT NOT NULL);
        INSERT INTO apps VALUES('allowed','Editor.exe',1,'\"ask\"');
        INSERT INTO apps VALUES('denied','Game.exe',0,'\"off\"');
        CREATE TABLE settings(id INTEGER PRIMARY KEY CHECK(id=1),data TEXT NOT NULL);
        PRAGMA user_version=1;").unwrap();
    let mut settings = serde_json::to_value(Settings::default()).unwrap();
    settings
        .as_object_mut()
        .unwrap()
        .remove("automatic_learning");
    old.execute("INSERT INTO settings VALUES(1,?1)", [settings.to_string()])
        .unwrap();
    drop(old);
    let store = Store::open(&path).unwrap();
    let allowed = store.permission("allowed").unwrap().unwrap();
    let denied = store.permission("denied").unwrap().unwrap();
    assert!(allowed.observe && allowed.explicit);
    assert!(!denied.observe && denied.explicit);
    assert!(store.settings().unwrap().automatic_learning);
    assert_eq!(store.event_count().unwrap(), 0);
    drop(store);
    assert!(Store::open(&path).is_ok());
}
