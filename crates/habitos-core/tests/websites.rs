use habitos_core::{
    browser_bridge,
    engine::Engine,
    model::*,
    platform::{DemoPlatform, Platform},
    store::Store,
    website,
};
const AT: i64 = 1_800_000_000;
fn setup() -> (Engine, DemoPlatform) {
    let mut p = DemoPlatform::default();
    p.demo_scene("browser").unwrap();
    let mut e = Engine::new(Store::memory().unwrap(), true).unwrap();
    e.observe(p.snapshot().unwrap(), AT).unwrap();
    let mut s = e.settings.clone();
    s.website_observation = true;
    e.save_settings(s).unwrap();
    (e, p)
}
#[test]
fn origins_strip_private_paths_and_reject_non_web_actions() {
    assert_eq!(
        website::origin("https://www.bilibili.com/video/BV123?token=private#x").unwrap(),
        "https://www.bilibili.com/"
    );
    for value in [
        "file:///C:/Windows/a.exe",
        "javascript:alert(1)",
        "http://example.com/",
        "https://user:secret@example.com/",
        "https://127.0.0.1/",
        "https://localhost/",
        "https://example.com:8443/",
        "https://box.local/",
    ] {
        assert!(website::origin(value).is_err(), "{value}");
    }
}
#[test]
fn visits_deduplicate_dates_and_auto_register_only_after_three() {
    let (mut e, _) = setup();
    let id = website::id("https://www.bilibili.com/");
    for n in 0..1000 {
        e.observe_website(
            "https://www.bilibili.com/video/private?secret=1",
            AT + n,
            AT + n,
        )
        .unwrap();
    }
    assert_eq!(
        e.store
            .usage_scores(AT + 1000, 4)
            .unwrap()
            .iter()
            .find(|s| s.app_id == id)
            .unwrap()
            .days,
        1
    );
    assert!(e.store.targets().unwrap().is_empty());
    for n in 1..=2 {
        e.current.as_mut().unwrap().local_day = Some(20261009 + n);
        e.observe_website("https://www.bilibili.com/", AT + 1000 + n, AT + 1000 + n)
            .unwrap();
    }
    assert_eq!(
        e.store.launch_path(&id).unwrap(),
        "https://www.bilibili.com/"
    );
    assert_eq!(e.status(AT + 1003).unwrap().quick_apps[0].kind, "website");
    assert_eq!(e.status(AT + 1003).unwrap().samples, 0);
}
#[test]
fn no_permission_pause_sensitive_or_stale_event_does_not_record() {
    let (mut e, _) = setup();
    let mut s = e.settings.clone();
    s.website_observation = false;
    e.save_settings(s).unwrap();
    assert!(e
        .observe_website("https://www.bilibili.com/", AT, AT)
        .is_err());
    let mut s = e.settings.clone();
    s.website_observation = true;
    e.save_settings(s).unwrap();
    assert!(e
        .observe_website("https://www.bilibili.com/", AT - 31, AT)
        .is_err());
    let id = e.current.as_ref().unwrap().app_id.clone();
    e.sensitive(&id, true).unwrap();
    assert!(e
        .observe_website("https://www.bilibili.com/", AT, AT)
        .is_err());
    assert!(e.store.targets().unwrap().is_empty());
    assert!(e.store.usage(AT).unwrap().is_empty());
}
#[test]
fn website_click_is_explicit_and_does_not_change_volume_or_fake_usage() {
    let (mut e, mut p) = setup();
    e.register_website("https://www.bilibili.com/video/123")
        .unwrap();
    let id = website::id("https://www.bilibili.com/");
    let session = e.current.as_ref().unwrap().session.clone();
    assert!(e.app_recommendation.is_none());
    assert!(e.open_target(&id, "old", &mut p, AT + 1).is_err());
    e.open_target(&id, &session, &mut p, AT + 1).unwrap();
    assert_eq!(p.current.volume, 0.5);
    assert!(e.store.usage(AT + 1).unwrap().is_empty());
    e.permission(&id, false, Mode::Off).unwrap();
    assert!(e.open_target(&id, &session, &mut p, AT + 2).is_err());
}
#[test]
fn deleting_website_memory_keeps_target_and_does_not_resurrect_score() {
    let (mut e, _) = setup();
    e.register_website("https://www.bilibili.com/").unwrap();
    e.observe_website("https://www.bilibili.com/", AT, AT)
        .unwrap();
    let id = website::id("https://www.bilibili.com/");
    e.delete_memory(&id).unwrap();
    assert!(e.store.usage_scores(AT, 4).unwrap().is_empty());
    assert_eq!(
        e.store.launch_path(&id).unwrap(),
        "https://www.bilibili.com/"
    );
}
#[test]
fn bounded_native_messages_and_inbox_never_persist_document_urls() {
    let dir = tempfile::tempdir().unwrap();
    let now = habitos_core::model::now();
    let json = serde_json::to_vec(&website::Visit {
        origin: "https://www.bilibili.com/private?secret=1".into(),
        at: now,
    })
    .unwrap();
    let mut framed = (json.len() as u32).to_le_bytes().to_vec();
    framed.extend(json);
    let v = website::read_message(&mut framed.as_slice()).unwrap();
    browser_bridge::enqueue(dir.path(), &v).unwrap();
    let mut got = vec![];
    browser_bridge::drain(dir.path(), |v| got.push(v));
    assert_eq!(got[0].origin, "https://www.bilibili.com/");
    assert_eq!(
        std::fs::read_dir(dir.path().join("browser-inbox"))
            .unwrap()
            .count(),
        0
    );
    assert!(website::read_message(&mut 9000u32.to_le_bytes().as_slice()).is_err());
    let mut out = vec![];
    website::reply(&mut out, true).unwrap();
    assert_eq!(
        u32::from_le_bytes(out[..4].try_into().unwrap()) as usize,
        out.len() - 4
    );
}
#[test]
fn configurable_suggestions_are_easier_but_never_grant_actions() {
    let s = Settings::default();
    assert_eq!(s.app_min_days, 2);
    assert_eq!(s.app_threshold, 0.50);
    assert_eq!(s.suggest_threshold, 0.50);
    assert_eq!(s.auto_threshold, 0.50);
    assert_eq!(s.max_auto_delta, 0.05);
    assert!(!s.website_observation);
    let mut bad = s.clone();
    bad.auto_threshold = 0.49;
    assert!(bad.validate().is_err());
    let mut bad = s.clone();
    bad.app_min_days = 1;
    assert!(bad.validate().is_err());
    let (mut e, mut p) = setup();
    e.demo_websites(AT).unwrap();
    assert!(e
        .quick_apps(AT)
        .unwrap()
        .iter()
        .any(|r| r.kind == "website"));
    assert_eq!(p.snapshot().unwrap().volume, 0.5);
}

#[test]
fn site_opt_out_is_exported_before_inbox_persistence_and_cannot_gain_volume_permission() {
    let (mut e, _) = setup();
    e.register_website("https://www.bilibili.com/").unwrap();
    let id = website::id("https://www.bilibili.com/");
    assert!(e.permission(&id, true, Mode::Auto).is_err());
    e.sensitive(&id, true).unwrap();
    let mut state = e.status(AT).unwrap();
    state.demo = false;
    let dir = tempfile::tempdir().unwrap();
    browser_bridge::write_status_policy(dir.path(), &state).unwrap();
    let policy: browser_bridge::Policy =
        serde_json::from_slice(&std::fs::read(dir.path().join("browser-policy.json")).unwrap())
            .unwrap();
    assert!(policy
        .blocked_domains
        .contains(&"www.bilibili.com".to_string()));
    assert!(policy.enabled);
    state.settings.paused = true;
    assert!(!browser_bridge::capture_enabled(&state));
}
#[test]
fn three_consistent_dates_can_prompt_without_auto_open_or_volume_change() {
    let (mut e, mut p) = setup();
    e.demo_websites(AT).unwrap();
    e.observe(p.snapshot().unwrap(), AT + 6).unwrap();
    let offer = e.app_recommendation.as_ref().unwrap();
    assert_eq!(offer.days, 3);
    assert!(offer.confidence >= 0.60);
    assert_eq!(p.snapshot().unwrap().volume, 0.5);
    assert!(e.status(AT + 6).unwrap().journal.is_empty());
}
