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
static SERIAL: Mutex<()> = Mutex::new(());
static IN_FLIGHT: Mutex<Option<Box<dyn FnOnce() + Send>>> = Mutex::new(None);
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
        let in_flight = super::IN_FLIGHT.lock().unwrap().take();
        if let Some(change) = in_flight {
            change();
        }
        // Deliberately return an old result even after a cancellation. The caller
        // must defend the commit boundary, not rely on a cooperative provider.
        super::RESPONSE.lock().unwrap().clone()
    }
}
#[path = "../../../src-tauri/src/cold_ai.rs"]
mod cold_ai;

#[test]
fn optional_ai_lifecycle_uses_real_gate_and_cached_memory_without_real_requests() {
    let _serial = SERIAL.lock().unwrap();
    CALLS.store(0, Ordering::SeqCst);
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

#[test]
fn late_manual_replies_are_discarded_after_pause_disable_or_reconfiguration() {
    let _serial = SERIAL.lock().unwrap();
    let dir = tempfile::tempdir().unwrap();
    for change in 0..3 {
        let rt = Runtime::start(dir.path().join(format!("manual-{change}.db")), true).unwrap();
        let layer = std::sync::Arc::new(cold_ai::ColdAi::default());
        layer
            .configure(
                true,
                "https://example.com/v1/chat/completions".into(),
                "old-model".into(),
                "old-key".into(),
            )
            .unwrap();
        *RESPONSE.lock().unwrap() = Ok("late answer".into());
        let action_layer = layer.clone();
        let action_rt = rt.clone();
        *IN_FLIGHT.lock().unwrap() = Some(Box::new(move || match change {
            0 => {
                let mut settings = action_rt.command(Command::Status).unwrap().settings;
                settings.paused = true;
                action_rt.command(Command::Settings(settings)).unwrap();
            }
            1 => action_layer
                .configure(false, String::new(), String::new(), String::new())
                .unwrap(),
            _ => action_layer
                .configure(
                    true,
                    "https://example.com/v1/chat/completions".into(),
                    "new-model".into(),
                    "new-key".into(),
                )
                .unwrap(),
        }));
        let reply = if change == 2 {
            layer.ask_at(
                &rt,
                "test question",
                "https://example.com/v1/chat/completions".into(),
                "old-model".into(),
                "old-key".into(),
            )
        } else {
            layer.ask(&rt, "test question")
        };
        assert!(reply.unwrap_err().contains("取消"));
        rt.shutdown();
    }
}

#[test]
fn late_automatic_advice_cannot_create_a_question_after_its_context_is_invalidated() {
    let _serial = SERIAL.lock().unwrap();
    let dir = tempfile::tempdir().unwrap();
    for change in 0..4 {
        let rt = Runtime::start(dir.path().join(format!("automatic-{change}.db")), true).unwrap();
        rt.command(Command::DemoCorrection).unwrap();
        let layer = std::sync::Arc::new(cold_ai::ColdAi::default());
        layer
            .configure(
                true,
                "https://example.com/v1/chat/completions".into(),
                "fixture".into(),
                "test-key".into(),
            )
            .unwrap();
        *RESPONSE.lock().unwrap() = Ok(
            r#"{"interpretation":"hold_app_device","app_id":null,"explanation":"old advice"}"#
                .into(),
        );
        let action_layer = layer.clone();
        let action_rt = rt.clone();
        *IN_FLIGHT.lock().unwrap() = Some(Box::new(move || match change {
            0 => {
                let mut settings = action_rt.command(Command::Status).unwrap().settings;
                settings.paused = true;
                action_rt.command(Command::Settings(settings)).unwrap();
            }
            1 => action_layer
                .configure(false, String::new(), String::new(), String::new())
                .unwrap(),
            2 => {
                action_rt
                    .command(Command::DemoScene("sensitive".into()))
                    .unwrap();
            }
            _ => action_layer
                .configure(
                    true,
                    "https://example.com/v1/chat/completions".into(),
                    "new-model".into(),
                    "new-key".into(),
                )
                .unwrap(),
        }));
        let baseline = CALLS.load(Ordering::SeqCst);
        layer.try_wake(&rt, false);
        assert_eq!(CALLS.load(Ordering::SeqCst), baseline + 1);
        assert!(rt
            .command(Command::Status)
            .unwrap()
            .experience
            .question
            .is_none());
        assert!(!layer.status().awake);
        layer.try_wake(&rt, false);
        assert_eq!(
            CALLS.load(Ordering::SeqCst),
            baseline + 1,
            "cancelled wake must not loop"
        );
        rt.shutdown();
    }
}
