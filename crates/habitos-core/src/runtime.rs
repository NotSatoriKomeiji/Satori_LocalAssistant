//! A blocking actor. Native events and finite deadlines wake it; no polling loop.
use crate::{
    engine::Engine,
    model::*,
    platform::{DemoPlatform, Platform, Signal, Waker},
    store::Store,
    Result,
};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicU8, Ordering},
        mpsc::{self, Receiver, Sender},
        Arc,
    },
    time::{Duration, Instant},
};
pub type Notifier = Arc<dyn Fn(Status) + Send + Sync>;
pub enum Command {
    Status,
    BrightnessCorrection {
        context: Context,
        device: String,
    },
    ClaimWake {
        id: String,
        revision: u64,
    },
    FinishWake {
        id: String,
        revision: u64,
        advice: Option<crate::experience::Advice>,
    },
    AnswerExperience {
        id: String,
        revision: u64,
        accept: bool,
    },
    RemoveExperience(String),
    ReserveManualAi,
    LocalExperienceHit,
    DemoCorrection,
    RegisterWebsite(String),
    BrowserVisit(crate::website::Visit),
    DemoWebsites,
    Refresh,
    GrantCurrent,
    Permission {
        id: String,
        observe: bool,
        mode: Mode,
    },
    Sensitive {
        id: String,
        sensitive: bool,
    },
    Capture,
    Accept(String),
    Reject(String),
    Undo(String),
    Settings(Settings),
    Forget,
    PinMemory {
        id: String,
        pinned: bool,
    },
    DeleteMemory(String),
    RegisterTarget(String),
    TargetEnabled {
        id: String,
        enabled: bool,
    },
    LaunchApp(String),
    OpenTarget {
        id: String,
        session: String,
    },
    Startup(bool),
    DismissApp(String),
    DemoRecommend,
    DemoGrid,
    DemoScene(String),
    DemoVolume(f64),
    DemoTeach,
    Notify(Notifier),
}
struct Request {
    command: Command,
    reply: Sender<Result<Status>>,
}
enum Envelope {
    Request(Box<Request>),
    Wake,
    Shutdown,
}
struct Control {
    sender: Sender<Envelope>,
}
impl Drop for Control {
    fn drop(&mut self) {
        let _ = self.sender.send(Envelope::Shutdown);
    }
}
#[derive(Clone)]
pub struct Runtime {
    control: Arc<Control>,
}
impl Runtime {
    pub fn start(path: PathBuf, demo: bool) -> Result<Self> {
        Self::start_worker(move || {
            let store = Store::open(&path)?;
            let platform: Box<dyn Platform> = if demo {
                Box::<DemoPlatform>::default()
            } else {
                #[cfg(windows)]
                {
                    Box::new(crate::platform::windows::WindowsPlatform::new()?)
                }
                #[cfg(not(windows))]
                {
                    return Err(crate::rule(
                        "真实事件监听仅支持 Windows；其他平台请使用 --demo",
                    ));
                }
            };
            Ok((Engine::new(store, demo)?, platform))
        })
    }
    fn start_worker(
        factory: impl FnOnce() -> Result<(Engine, Box<dyn Platform>)> + Send + 'static,
    ) -> Result<Self> {
        let (sender, receiver) = mpsc::channel();
        let (ready, ready_rx) = mpsc::channel();
        let pending = Arc::new(AtomicU8::new(0));
        let signal_pending = pending.clone();
        let event_sender = sender.clone();
        let wake: Waker = Arc::new(move |signal| {
            if signal_pending.fetch_or(signal as u8, Ordering::AcqRel) == 0 {
                let _ = event_sender.send(Envelope::Wake);
            }
        });
        std::thread::Builder::new()
            .name("habitos-runtime".into())
            .spawn(move || {
                let setup = (|| -> Result<(Engine, Box<dyn Platform>)> {
                    let (mut engine, mut platform) = factory()?;
                    platform.subscribe(wake)?;
                    platform.set_paused(engine.settings.paused)?;
                    engine.startup = platform.startup_status().unwrap_or_default();
                    Ok((engine, platform))
                })();
                match setup {
                    Ok((engine, platform)) => {
                        let _ = ready.send(Ok(()));
                        run(receiver, pending, engine, platform);
                    }
                    Err(error) => {
                        let _ = ready.send(Err(error));
                    }
                }
            })
            .map_err(|_| crate::rule("无法启动后台线程"))?;
        ready_rx
            .recv()
            .map_err(|_| crate::rule("后台线程未启动"))??;
        Ok(Self {
            control: Arc::new(Control { sender }),
        })
    }
    pub fn command(&self, command: Command) -> Result<Status> {
        let (reply, receiver) = mpsc::channel();
        self.control
            .sender
            .send(Envelope::Request(Box::new(Request { command, reply })))
            .map_err(|_| crate::rule("后台线程已停止"))?;
        receiver
            .recv_timeout(Duration::from_secs(10))
            .map_err(|_| crate::rule("后台响应超时"))?
    }
    pub fn on_change(&self, notify: Notifier) -> Result<()> {
        self.command(Command::Notify(notify))?;
        Ok(())
    }
    pub fn shutdown(&self) {
        let _ = self.control.sender.send(Envelope::Shutdown);
    }
}
fn refresh(engine: &mut Engine, platform: &mut dyn Platform) -> Result<()> {
    if engine.settings.paused {
        return Ok(());
    }
    match platform.snapshot() {
        Ok(snapshot) => engine.observe(snapshot, now()),
        Err(error) => {
            engine.clear_context();
            Err(error)
        }
    }
}
fn decide(engine: &mut Engine, platform: &mut dyn Platform) {
    if let Err(error) = refresh(engine, platform) {
        engine.message = error.to_string();
    }
    if engine.auto_eligible().unwrap_or(false) {
        if let Some(p) = engine.proposal.clone() {
            if let Err(error) = engine.apply(&p.id, true, platform, now()) {
                engine.message = error.to_string();
                engine.proposal = None;
            }
        }
    }
}
fn publish(engine: &Engine, notify: &Option<Notifier>) {
    if let Some(notify) = notify {
        if let Ok(status) = engine.status(now()) {
            notify(status);
        }
    }
}
fn run(
    receiver: Receiver<Envelope>,
    pending: Arc<AtomicU8>,
    mut engine: Engine,
    mut platform: Box<dyn Platform>,
) {
    let mut notify = None;
    let mut debounce = None;
    let mut flags = 0u8;
    let mut clock = Instant::now() + platform.next_clock_in();
    let mut cleanup = Instant::now() + Duration::from_secs(86400);
    decide(&mut engine, platform.as_mut());
    loop {
        let instant = Instant::now();
        if debounce.is_some_and(|due: Instant| instant >= due) {
            debounce = None;
            if !engine.settings.paused {
                if flags & Signal::Foreground as u8 != 0 {
                    engine.request_decision();
                }
                decide(&mut engine, platform.as_mut());
                for (bit, name) in [
                    (Signal::Volume as u8, "volume_change"),
                    (Signal::Device as u8, "device_change"),
                ] {
                    if flags & bit != 0 {
                        if let Err(error) = engine.log_signal(name, now()) {
                            engine.message = error.to_string();
                        }
                    }
                }
                publish(&engine, &notify);
            }
            flags = 0;
        }
        if !engine.settings.paused && instant >= clock {
            engine.request_decision();
            decide(&mut engine, platform.as_mut());
            if let Err(error) = engine.log_signal("time_node", now()) {
                engine.message = error.to_string();
            }
            clock = Instant::now() + platform.next_clock_in();
            publish(&engine, &notify);
        }
        if engine.next_deadline().is_some_and(|due| now() >= due) {
            decide(&mut engine, platform.as_mut());
            publish(&engine, &notify);
        }
        if instant >= cleanup {
            if let Err(error) = engine.store.prune(now(), engine.settings.retention_days) {
                engine.message = error.to_string();
            }
            cleanup = Instant::now() + Duration::from_secs(86400);
        }
        let mut due = cleanup;
        if !engine.settings.paused {
            due = due.min(clock);
        }
        if let Some(debounce) = debounce {
            due = due.min(debounce);
        }
        if let Some(deadline) = engine.next_deadline() {
            due = due.min(
                Instant::now()
                    + Duration::from_secs((deadline - now()).max(0) as u64)
                    + Duration::from_millis(50),
            );
        }
        match receiver.recv_timeout(due.saturating_duration_since(Instant::now())) {
            Ok(Envelope::Shutdown) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Ok(Envelope::Wake) => {
                flags |= pending.swap(0, Ordering::AcqRel);
                if debounce.is_none() {
                    debounce = Some(Instant::now() + Duration::from_millis(100));
                }
            }
            Ok(Envelope::Request(request)) => {
                let result = match request.command {
                    Command::Notify(callback) => {
                        notify = Some(callback);
                        Ok(())
                    }
                    command => handle(command, &mut engine, platform.as_mut()),
                }
                .and_then(|_| engine.status(now()));
                if let Err(error) = &result {
                    engine.message = error.to_string();
                }
                let _ = request.reply.send(result);
                publish(&engine, &notify);
                if engine.settings.paused {
                    flags = 0;
                    debounce = None;
                } else if clock < Instant::now() {
                    clock = Instant::now() + platform.next_clock_in();
                }
            }
        }
    }
}
fn handle(command: Command, engine: &mut Engine, platform: &mut dyn Platform) -> Result<()> {
    match command {
        Command::BrightnessCorrection { context, device } => {
            engine.brightness_correction(context, device, now())
        }
        Command::ClaimWake { id, revision } => engine.claim_wake(&id, revision, now()),
        Command::FinishWake {
            id,
            revision,
            advice,
        } => engine.finish_wake(&id, revision, advice),
        Command::AnswerExperience {
            id,
            revision,
            accept,
        } => engine.answer_experience(&id, revision, accept, platform, now()),
        Command::RemoveExperience(id) => engine.store.remove_experience(&id),
        Command::ReserveManualAi => {
            if engine.settings.paused {
                Err(crate::rule("助手已暂停"))
            } else {
                engine.store.reserve_manual_ai(now())
            }
        }
        Command::LocalExperienceHit => engine.store.local_experience_hit(),
        Command::DemoCorrection => engine.demo_correction(now()),
        Command::Status => {
            engine.startup = platform
                .startup_status()
                .unwrap_or_else(|error| StartupStatus {
                    message: error.to_string(),
                    ..StartupStatus::default()
                });
            Ok(())
        }
        Command::Refresh => {
            engine.request_decision();
            refresh(engine, platform)
        }
        Command::GrantCurrent => {
            refresh(engine, platform)?;
            engine.grant_current()
        }
        Command::Permission { id, observe, mode } => engine.permission(&id, observe, mode),
        Command::Sensitive { id, sensitive } => engine.sensitive(&id, sensitive),
        Command::Capture => {
            refresh(engine, platform)?;
            engine.capture(now(), "explicit")
        }
        Command::Accept(id) => engine.apply(&id, false, platform, now()),
        Command::Reject(id) => engine.reject(&id, now()),
        Command::Undo(id) => engine.undo(&id, platform, now()),
        Command::Settings(settings) => {
            // Unsubscribe before committing pause. Resume failure keeps the previous settings.
            let paused = settings.paused;
            platform.set_paused(paused)?;
            if let Err(error) = engine.save_settings(settings) {
                let _ = platform.set_paused(engine.settings.paused);
                return Err(error);
            }
            engine.clear_context();
            if !paused {
                refresh(engine, platform)?;
            }
            Ok(())
        }
        Command::Forget => engine.forget(),
        Command::PinMemory { id, pinned } => engine.pin_memory(&id, pinned),
        Command::DeleteMemory(id) => engine.delete_memory(&id),
        Command::RegisterTarget(path) => engine.register_target(&path),
        Command::TargetEnabled { id, enabled } => engine.target_enabled(&id, enabled),
        Command::LaunchApp(id) => engine.launch_app(&id, platform, now()),
        Command::OpenTarget { id, session } => engine.open_target(&id, &session, platform, now()),
        Command::Startup(enabled) => {
            platform.set_startup(enabled)?;
            engine.startup = platform.startup_status()?;
            Ok(())
        }
        Command::DismissApp(id) => engine.dismiss_app(&id, now()),
        Command::DemoRecommend => engine.demo_recommend(now()),
        Command::RegisterWebsite(url) => engine.register_website(&url),
        Command::BrowserVisit(visit) => {
            refresh(engine, platform)?;
            engine.observe_website(&visit.origin, visit.at, now())
        }
        Command::DemoWebsites => engine.demo_websites(now()),
        Command::DemoGrid => engine.demo_grid(now()),
        Command::DemoScene(scene) => {
            platform.demo_scene(&scene)?;
            engine.request_decision();
            refresh(engine, platform)
        }
        Command::DemoVolume(volume) => {
            platform.demo_volume(volume)?;
            refresh(engine, platform)
        }
        Command::DemoTeach => engine.demo_teach(now()),
        Command::Notify(_) => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{atomic::AtomicUsize, Mutex};
    #[test]
    fn idle_actor_and_panel_status_do_not_poll_devices_but_events_wake_it() {
        struct Spy {
            demo: DemoPlatform,
            reads: Arc<AtomicUsize>,
            wake: Arc<Mutex<Option<Waker>>>,
        }
        impl Platform for Spy {
            fn snapshot(&mut self) -> Result<crate::platform::Snapshot> {
                self.reads.fetch_add(1, Ordering::SeqCst);
                self.demo.snapshot()
            }
            fn set_volume(&mut self, s: &crate::platform::Snapshot, v: f64) -> Result<()> {
                self.demo.set_volume(s, v)
            }
            fn subscribe(&mut self, w: Waker) -> Result<()> {
                *self.wake.lock().unwrap() = Some(w);
                Ok(())
            }
        }
        let reads = Arc::new(AtomicUsize::new(0));
        let wake = Arc::new(Mutex::new(None::<Waker>));
        let spy_reads = reads.clone();
        let spy_wake = wake.clone();
        let runtime = Runtime::start_worker(move || {
            Ok((
                Engine::new(Store::memory()?, true)?,
                Box::new(Spy {
                    demo: DemoPlatform::default(),
                    reads: spy_reads,
                    wake: spy_wake,
                }),
            ))
        })
        .unwrap();
        runtime.command(Command::Status).unwrap();
        let baseline = reads.load(Ordering::SeqCst);
        std::thread::sleep(Duration::from_millis(550));
        for _ in 0..4 {
            runtime.command(Command::Status).unwrap();
        }
        assert_eq!(
            reads.load(Ordering::SeqCst),
            baseline,
            "no 250ms/2s device polling"
        );
        wake.lock().unwrap().as_ref().unwrap()(Signal::Foreground);
        std::thread::sleep(Duration::from_millis(220));
        runtime.command(Command::Status).unwrap();
        assert!(reads.load(Ordering::SeqCst) > baseline);
        runtime.shutdown();
    }
}
