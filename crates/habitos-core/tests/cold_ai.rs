//! Exercises the real optional-layer code with an injected transport: no paid API.
use habitos_core::{
    model::now,
    runtime::{Command, Runtime},
};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Mutex,
};
static CALLS: AtomicUsize = AtomicUsize::new(0);
static INPUT: Mutex<String> = Mutex::new(String::new());
static RESPONSE: Mutex<Result<String, String>> = Mutex::new(Err(String::new()));
mod assist {
    pub fn powershell_cancel(
        _: &str,
        input: &serde_json::Value,
        cancel: impl Fn() -> bool,
    ) -> Result<String, String> {
        if cancel() {
            return Err("cancelled".into());
        }
        super::CALLS.fetch_add(1, super::Ordering::SeqCst);
        *super::INPUT.lock().unwrap() = input["prompt"].as_str().unwrap().into();
        super::RESPONSE.lock().unwrap().clone()
    }
}
#[path = "../../../src-tauri/src/cold_ai.rs"]
mod cold_ai;

#[test]
fn optional_ai_lifecycle_uses_real_gate_and_cached_memory_without_real_requests() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("habitos.sqlite3");
    let rt = Runtime::start(path.clone(), true).unwrap();
    let layer = cold_ai::ColdAi::default();
    assert!(
        layer.ask(&rt, "问题").is_err(),
        "manual asking cannot call a disabled optional layer"
    );
    // API off: even unresolved repeated evidence cannot touch transport.
    rt.command(Command::DemoCorrection).unwrap();
    layer.try_wake(&rt, false);
    assert_eq!(CALLS.load(Ordering::SeqCst), 0);
    let st = rt.command(Command::Status).unwrap();
    assert_eq!(st.experience.rules.len(), 1);
    assert_eq!(st.experience.automatic_calls_today, 0);
    let e = st.experience.pending[0].clone();
    layer
        .configure(
            true,
            "https://example.com/v1/chat/completions".into(),
            "test".into(),
            "test-secret".into(),
        )
        .unwrap();
    // Pause and sensitive foreground stop wakes independently of provider configuration.
    let mut settings = st.settings;
    settings.paused = true;
    rt.command(Command::Settings(settings.clone())).unwrap();
    layer.try_wake(&rt, false);
    assert_eq!(CALLS.load(Ordering::SeqCst), 0);
    settings.paused = false;
    rt.command(Command::Settings(settings)).unwrap();
    rt.command(Command::DemoScene("sensitive".into())).unwrap();
    layer.try_wake(&rt, false);
    assert_eq!(CALLS.load(Ordering::SeqCst), 0);
    rt.command(Command::DemoScene("game".into())).unwrap();
    *RESPONSE.lock().unwrap()=Ok(r#"{"interpretation":"hold_app_device","app_id":null,"explanation":"是否在这个软件使用当前设备时保持手动音量？"}"#.into());
    layer.try_wake(&rt, false);
    assert_eq!(CALLS.load(Ordering::SeqCst), 1);
    let st = rt.command(Command::Status).unwrap();
    assert!(st.experience.question.is_some());
    assert!(!st.experience.rules[0].broad);
    let payload = INPUT.lock().unwrap().clone();
    assert!(!payload.contains("test-secret"));
    assert!(!payload.contains(&e.scope.device));
    assert!(!payload.contains(&e.scope.app_id));
    rt.command(Command::AnswerExperience {
        id: e.id.clone(),
        revision: e.revision,
        accept: true,
    })
    .unwrap();
    assert!(rt.command(Command::Status).unwrap().experience.rules[0].broad);
    for _ in 0..4 {
        layer.try_wake(&rt, false);
    }
    assert_eq!(
        CALLS.load(Ordering::SeqCst),
        1,
        "learned situations never re-query"
    );
    // Disable removes credentials; memory stays available offline and survives restart.
    layer
        .configure(false, String::new(), String::new(), String::new())
        .unwrap();
    let offline = habitos_core::store::Store::open(&path).unwrap();
    assert!(offline
        .holds_adjustment(habitos_core::experience::Kind::Volume, &e.scope, now())
        .unwrap());
    assert_eq!(
        offline
            .experience_status(None, now())
            .unwrap()
            .automatic_calls_today,
        1
    );
    drop(offline);
    rt.command(Command::Forget).unwrap();
    assert!(rt
        .command(Command::Status)
        .unwrap()
        .experience
        .rules
        .is_empty());
    assert_eq!(
        rt.command(Command::Status)
            .unwrap()
            .experience
            .automatic_calls_today,
        1,
        "clearing memory never resets request budget"
    );
    rt.shutdown();
    // A provider failure cannot take away the local exception or create a retry loop.
    let failed_rt = Runtime::start(dir.path().join("failed.sqlite3"), true).unwrap();
    failed_rt.command(Command::DemoCorrection).unwrap();
    layer
        .configure(
            true,
            "https://example.com/v1/chat/completions".into(),
            "test".into(),
            "test-secret".into(),
        )
        .unwrap();
    *RESPONSE.lock().unwrap() = Err("timeout".into());
    let baseline = CALLS.load(Ordering::SeqCst);
    layer.try_wake(&failed_rt, false);
    assert_eq!(CALLS.load(Ordering::SeqCst), baseline + 1);
    for _ in 0..3 {
        layer.try_wake(&failed_rt, false);
    }
    let failed = failed_rt.command(Command::Status).unwrap();
    assert_eq!(CALLS.load(Ordering::SeqCst), baseline + 1);
    assert!(failed.experience.rules[0].active);
    assert_eq!(failed.experience.rules[0].wake, "failed");
    assert!(!layer.status().awake);
    failed_rt.shutdown();
}
