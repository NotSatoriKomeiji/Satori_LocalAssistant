//! Simple local assistant API. No generic task executor or AI action plan.
use crate::{
    cold_ai::ColdAi,
    monitors::{self, Monitor},
};
use habitos_core::{
    adjustment::Choice,
    assistant_state::Saved,
    experience::{Kind, Scope},
    model::{now, Status},
    runtime::{Command, Runtime},
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    io::{BufRead, Read, Write},
    path::PathBuf,
    process::{Command as Process, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use tauri::State;
struct Data {
    saved: Saved,
    monitors: Vec<Monitor>,
    message: String,
    input_message: String,
    last_brightness_action: HashMap<String, (habitos_core::model::Context, u8, i64)>,
}
pub struct Assist {
    data: Mutex<Data>,
    device_lock: Mutex<()>,
    path: PathBuf,
    demo: bool,
    input_running: AtomicBool,
    ai: ColdAi,
}
impl Assist {
    pub fn new(path: PathBuf, demo: bool) -> Result<Self, String> {
        let saved = Saved::load(&path)?;
        Ok(Self {
            path,
            demo,
            input_running: AtomicBool::new(false),
            ai: ColdAi::default(),
            device_lock: Mutex::new(()),
            data: Mutex::new(Data {
                saved,
                monitors: if demo {
                    vec![Monitor {
                        id: "demo:display".into(),
                        name: "模拟外接显示器".into(),
                        value: 50,
                    }]
                } else {
                    vec![]
                },
                message: "正在了解你的亮度习惯".into(),
                input_message: "快速词已关闭".into(),
                last_brightness_action: HashMap::new(),
            }),
        })
    }
    fn update_saved(
        &self,
        d: &mut Data,
        edit: impl FnOnce(&mut Saved) -> Result<bool, String>,
    ) -> Result<bool, String> {
        d.saved.update(&self.path, edit)
    }
    pub fn start(self: &Arc<Self>, runtime: Runtime) {
        let ai_weak = Arc::downgrade(self);
        let ai_runtime = runtime.clone();
        std::thread::spawn(move || loop {
            let Some(s) = ai_weak.upgrade() else {
                break;
            };
            s.ai.try_wake(&ai_runtime, s.demo);
            drop(s);
            std::thread::sleep(Duration::from_secs(1));
        });
        let weak = Arc::downgrade(self);
        let rt = runtime.clone();
        std::thread::spawn(move || {
            let mut retry = 0;
            loop {
                std::thread::sleep(Duration::from_secs(1));
                let Some(s) = weak.upgrade() else {
                    break;
                };
                if s.demo {
                    continue;
                }
                let status = status(&rt).ok();
                let allowed = status.as_ref().is_some_and(ordinary);
                let enabled = s
                    .data
                    .lock()
                    .map(|d| d.saved.quick_words.enabled)
                    .unwrap_or(false);
                let words = s
                    .data
                    .lock()
                    .map(|d| d.saved.quick_words.ranked(now()))
                    .unwrap_or_default();
                // Short-lived browser policy: disabled on pause or loss of ordinary context.
                let live = json!({"enabled":enabled&&allowed,"at":now(),"words":words});
                let _ = std::fs::write(
                    s.path.with_file_name("quick-words-live.json"),
                    live.to_string(),
                );
                let inbox = s.path.with_file_name("quick-words-inbox");
                if let Ok(entries) = std::fs::read_dir(&inbox) {
                    for e in entries.flatten().take(64) {
                        let p = e.path();
                        if p.extension().is_some_and(|e| e == "json") {
                            if enabled && allowed {
                                if let Ok(bytes) = std::fs::read(&p) {
                                    if bytes.len() < 2048 {
                                        if let Ok(v) = serde_json::from_slice::<Value>(&bytes) {
                                            if let (Some(text), Some(at)) =
                                                (v["text"].as_str(), v["at"].as_i64())
                                            {
                                                if (0..=3).contains(&(now() - at)) {
                                                    s.learn(text);
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            let _ = std::fs::remove_file(p);
                        }
                    }
                }
                if enabled && !s.input_running.load(Ordering::Acquire) && now() >= retry {
                    retry = now() + 60;
                    let copy = s.clone();
                    let r = rt.clone();
                    s.input_running.store(true, Ordering::Release);
                    std::thread::spawn(move || {
                        let result = desktop_input(&copy, &r);
                        if let Err(e) = result {
                            if let Ok(mut d) = copy.data.lock() {
                                d.input_message = e;
                            }
                        }
                        copy.input_running.store(false, Ordering::Release);
                    });
                }
            }
        });
        let weak = Arc::downgrade(self);
        std::thread::spawn(move || {
            let mut last = HashMap::<String, (u8, String)>::new();
            let mut acted = HashMap::<String, String>::new();
            loop {
                let Some(s) = weak.upgrade() else {
                    break;
                };
                if !s.demo {
                    let _ = brightness_tick(&s, &runtime, &mut last, &mut acted);
                }
                drop(s);
                std::thread::sleep(Duration::from_secs(15));
            }
        });
    }
    fn learn(&self, text: &str) {
        if let Ok(mut d) = self.data.lock() {
            if let Err(e) =
                self.update_saved(&mut d, |saved| Ok(saved.quick_words.learn(text, now())))
            {
                d.input_message = e;
            }
        }
    }
}
fn status(rt: &Runtime) -> Result<Status, String> {
    rt.command(Command::Status).map_err(|e| e.to_string())
}
fn ordinary(s: &Status) -> bool {
    !s.settings.paused
        && s.settings.automatic_learning
        && s.current.as_ref().is_some_and(|c| {
            s.apps
                .iter()
                .any(|a| a.id == c.app_id && a.observe && !a.sensitive)
        })
}
fn brightness_tick(
    s: &Assist,
    rt: &Runtime,
    last: &mut HashMap<String, (u8, String)>,
    acted: &mut HashMap<String, String>,
) -> Result<(), String> {
    let st = status(rt)?;
    if !ordinary(&st) {
        last.clear();
        return Ok(());
    }
    let c = st.current.as_ref().ok_or("没有场景")?;
    let _guard = s.device_lock.lock().map_err(|_| "设备繁忙")?;
    let rows = monitors::read()?;
    let mut d = s.data.lock().map_err(|_| "状态不可用")?;
    d.monitors = rows.clone();
    d.last_brightness_action
        .retain(|_, (_, _, at)| (0..=300).contains(&(now() - *at)));
    if rows.is_empty() {
        d.message = "未发现可调节的外接屏；请检查屏幕菜单中的 DDC/CI".into();
    }
    for m in rows {
        let previous = last.get(&m.id).cloned();
        if let Some((value, session)) = previous {
            if value.abs_diff(m.value) > 1 {
                acted.insert(m.id.clone(), c.session.clone());
                if session == c.session {
                    if let Some((context, after, at)) = d.last_brightness_action.remove(&m.id) {
                        if context.session == c.session
                            && (0..=300).contains(&(now() - at))
                            && after.abs_diff(m.value) > 1
                        {
                            rt.command(Command::BrightnessCorrection {
                                context,
                                device: m.id.clone(),
                            })
                            .map_err(|e| e.to_string())?;
                        }
                    }
                    s.update_saved(&mut d, |saved| {
                        saved.brightness.record(Choice {
                            app: c.app_id.clone(),
                            device: m.id.clone(),
                            period: c.hour / 6,
                            value: m.value,
                            at: now(),
                            session: c.session.clone(),
                        });
                        Ok(true)
                    })?;
                    d.message = "已记住你的手动亮度，本场景不再自动改动".into();
                }
            }
        }
        last.insert(m.id.clone(), (m.value, c.session.clone()));
        let scope = Scope::from_context(c, &m.id);
        let held = st
            .experience
            .rules
            .iter()
            .any(|e| e.kind == Kind::Brightness && e.scope.matches(&scope, e.broad));
        if held && acted.get(&m.id) != Some(&c.session) {
            acted.insert(m.id.clone(), c.session.clone());
            let _ = rt.command(Command::LocalExperienceHit);
            d.message = "记住了，这个场景保持你手动设置的亮度".into();
        }
        if st.settings.auto_adjustments && !held && acted.get(&m.id) != Some(&c.session) {
            if let Some(target) =
                d.saved
                    .brightness
                    .target(&c.app_id, &m.id, c.hour, m.value, &c.session, now())
            {
                acted.insert(m.id.clone(), c.session.clone());
                let check = status(rt)?;
                if !ordinary(&check)
                    || !check.settings.auto_adjustments
                    || check
                        .current
                        .as_ref()
                        .is_none_or(|x| x.session != c.session)
                {
                    continue;
                }
                match monitors::set(&m.id, m.value, target) {
                    Ok(()) => {
                        d.last_brightness_action
                            .insert(m.id.clone(), (c.clone(), target, now()));
                        last.insert(m.id.clone(), (target, c.session.clone()));
                        if let Some(row) = d.monitors.iter_mut().find(|r| r.id == m.id) {
                            row.value = target;
                        }
                        d.message = format!("已按习惯微调外接屏：{}% → {}%", m.value, target);
                    }
                    Err(e) => {
                        last.remove(&m.id);
                        d.message = e;
                    }
                }
            }
        }
    }
    Ok(())
}
#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Request {
    Status,
    QuickWords {
        enabled: bool,
    },
    Limit {
        limit: usize,
    },
    AddWord {
        text: String,
    },
    DeleteWord {
        text: String,
    },
    BrightnessRead,
    BrightnessSet {
        device: String,
        before: u8,
        value: u8,
    },
    Ai {
        endpoint: String,
        model: String,
        key: String,
        prompt: String,
    },
    AiMode {
        enabled: bool,
        endpoint: String,
        model: String,
        key: String,
    },
    ExperienceAnswer {
        id: String,
        revision: u64,
        accept: bool,
    },
    ExperienceForget {
        id: String,
    },
    DemoCorrection,
    Ask {
        prompt: String,
    },
}
#[tauri::command]
pub async fn assist_api(
    state: State<'_, Arc<Assist>>,
    runtime: State<'_, Runtime>,
    request: Request,
) -> Result<Value, String> {
    let s = state.inner().clone();
    let rt = runtime.inner().clone();
    tauri::async_runtime::spawn_blocking(move || handle(&s, &rt, request))
        .await
        .map_err(|_| "助手接口异常".to_string())?
}
fn snapshot(s: &Assist) -> Result<Value, String> {
    let d = s.data.lock().map_err(|_| "状态不可用")?;
    Ok(
        json!({"enabled":d.saved.quick_words.enabled,"limit":d.saved.quick_words.limit,"words":d.saved.quick_words.ranked(now()),"monitors":d.monitors,"message":d.message,"input_message":d.input_message,"running":s.input_running.load(Ordering::Acquire),"demo":s.demo,"ai":s.ai.status()}),
    )
}
fn handle(s: &Assist, rt: &Runtime, r: Request) -> Result<Value, String> {
    match r {
        Request::Ask { prompt } => {
            if s.demo {
                return Err("演示模式不会发送 AI 请求".into());
            }
            Ok(json!({"answer":s.ai.ask(rt,&prompt)?}))
        }
        Request::AiMode {
            enabled,
            endpoint,
            model,
            key,
        } => {
            if enabled && s.demo {
                return Err("演示模式不会连接 AI 服务".into());
            }
            s.ai.configure(enabled, endpoint, model, key)?;
            snapshot(s)
        }
        Request::ExperienceAnswer {
            id,
            revision,
            accept,
        } => {
            if accept && !s.demo {
                let st = status(rt)?;
                if let Some(e) = st
                    .experience
                    .question
                    .as_ref()
                    .filter(|e| e.id == id && e.kind == Kind::Brightness)
                {
                    let _guard = s.device_lock.lock().map_err(|_| "设备繁忙")?;
                    if !monitors::read()?.iter().any(|m| m.id == e.scope.device) {
                        return Err("外接显示器已变化，这条学习建议已失效".into());
                    }
                }
            }
            rt.command(Command::AnswerExperience {
                id,
                revision,
                accept,
            })
            .map_err(|e| e.to_string())?;
            snapshot(s)
        }
        Request::ExperienceForget { id } => {
            rt.command(Command::RemoveExperience(id))
                .map_err(|e| e.to_string())?;
            snapshot(s)
        }
        Request::DemoCorrection => {
            rt.command(Command::DemoCorrection)
                .map_err(|e| e.to_string())?;
            snapshot(s)
        }
        Request::Status => snapshot(s),
        Request::QuickWords { enabled } => {
            let mut d = s.data.lock().map_err(|_| "状态不可用")?;
            s.update_saved(&mut d, |saved| {
                saved.quick_words.enabled = enabled;
                Ok(true)
            })?;
            d.input_message = if enabled {
                "正在启动快速词；浏览器请安装配套扩展"
            } else {
                "快速词已关闭，停止学习和推荐"
            }
            .into();
            drop(d);
            snapshot(s)
        }
        Request::Limit { limit } => {
            if !(1..=200).contains(&limit) {
                return Err("最多保存1–200条快速词".into());
            }
            let mut d = s.data.lock().map_err(|_| "状态不可用")?;
            s.update_saved(&mut d, |saved| {
                saved.quick_words.limit = limit;
                Ok(true)
            })?;
            drop(d);
            snapshot(s)
        }
        Request::AddWord { text } => {
            let text = text.trim();
            if !habitos_core::quick_words::safe_text(text) {
                return Err("请添加2–120字的普通词句，避开号码、账号和密码".into());
            }
            let mut d = s.data.lock().map_err(|_| "状态不可用")?;
            s.update_saved(&mut d, |saved| {
                if saved.quick_words.words.iter().any(|w| w.text == text) {
                    return Ok(false);
                }
                {
                    if saved.quick_words.words.iter().filter(|w| w.pinned).count()
                        >= saved.quick_words.limit
                    {
                        return Err("已达到保存上限".into());
                    }
                    saved.quick_words.dismissed.retain(|w| w != text.trim());
                    saved
                        .quick_words
                        .words
                        .push(habitos_core::quick_words::Word {
                            text: text.into(),
                            uses: 2,
                            last: now(),
                            pinned: true,
                        });
                }
                Ok(true)
            })?;
            drop(d);
            snapshot(s)
        }
        Request::DeleteWord { text } => {
            let mut d = s.data.lock().map_err(|_| "状态不可用")?;
            s.update_saved(&mut d, |saved| {
                saved.quick_words.dismiss(&text);
                Ok(true)
            })?;
            drop(d);
            snapshot(s)
        }
        Request::BrightnessRead => {
            let _guard = s.device_lock.lock().map_err(|_| "设备繁忙")?;
            if !s.demo {
                let rows = monitors::read()?;
                let mut d = s.data.lock().map_err(|_| "状态不可用")?;
                d.monitors = rows;
                d.message = if d.monitors.is_empty() {
                    "没有检测到可调节的外接屏，请检查 DDC/CI"
                } else {
                    "已连接外接屏；手动调整会作为学习样本"
                }
                .into();
            }
            snapshot(s)
        }
        Request::BrightnessSet {
            device,
            before,
            value,
        } => {
            let _guard = s.device_lock.lock().map_err(|_| "设备繁忙")?;
            let st = status(rt)?;
            if st.settings.paused {
                return Err("助手已暂停".into());
            }
            if !(10..=100).contains(&value) {
                return Err("亮度范围10–100".into());
            }
            if !s.demo {
                monitors::set(&device, before, value)?;
            }
            let mut d = s.data.lock().map_err(|_| "状态不可用")?;
            let m = d
                .monitors
                .iter_mut()
                .find(|m| m.id == device)
                .ok_or("显示器已变化，请刷新")?;
            if s.demo && m.value != before {
                return Err("亮度已经变化，请刷新".into());
            }
            m.value = value;
            if let Some((context, after, at)) = d.last_brightness_action.remove(&device) {
                if ordinary(&st)
                    && st
                        .current
                        .as_ref()
                        .is_some_and(|c| c.session == context.session)
                    && (0..=300).contains(&(now() - at))
                    && after.abs_diff(value) > 1
                {
                    rt.command(Command::BrightnessCorrection {
                        context,
                        device: device.clone(),
                    })
                    .map_err(|e| e.to_string())?;
                }
            }
            if ordinary(&st) {
                if let Some(c) = st.current {
                    s.update_saved(&mut d, |saved| {
                        saved.brightness.record(Choice {
                            app: c.app_id,
                            device,
                            period: c.hour / 6,
                            value,
                            at: now(),
                            session: c.session,
                        });
                        Ok(true)
                    })
                    .map_err(|e| format!("亮度已调整，但学习数据未保存：{e}"))?;
                }
            }
            d.message = "已调整，并记住这次选择".into();
            drop(d);
            snapshot(s)
        }
        Request::Ai {
            endpoint,
            model,
            key,
            prompt,
        } => {
            if !s.ai.status().enabled {
                return Err("请先开启可选 AI 增强".into());
            }
            if s.demo {
                return Err("预览不会发送AI请求".into());
            }
            if status(rt)?.settings.paused {
                return Err("助手已暂停".into());
            }
            let text = s.ai.ask_at(rt, &prompt, endpoint, model, key)?;
            Ok(json!({"answer":text}))
        }
    }
}
fn desktop_input(s: &Arc<Assist>, rt: &Runtime) -> Result<(), String> {
    let mut cmd = Process::new("powershell.exe");
    cmd.args([
        "-NoLogo",
        "-NoProfile",
        "-Sta",
        "-Command",
        include_str!("../scripts/desktop-input.ps1"),
    ])
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }
    let mut child = cmd
        .spawn()
        .map_err(|_| "快速词未能启动；请检查Windows PowerShell")?;
    let mut stdin = child.stdin.take().ok_or("输入管道不可用")?;
    let stdout = child.stdout.take().ok_or("输出管道不可用")?;
    let weak = Arc::downgrade(s);
    let reader_runtime = rt.clone();
    std::thread::spawn(move || {
        for line in std::io::BufReader::new(stdout)
            .lines()
            .map_while(Result::ok)
        {
            let Some(s) = weak.upgrade() else {
                break;
            };
            if line == "ready" {
                if let Ok(mut d) = s.data.lock() {
                    d.input_message = "已开启：在普通输入框输入，右下角点击快速词".into();
                }
            } else if line.len() < 2048 {
                if let Ok(v) = serde_json::from_str::<Value>(&line) {
                    if let Some(text) = v["text"].as_str() {
                        if status(&reader_runtime).ok().is_some_and(|s| ordinary(&s)) {
                            s.learn(text);
                        }
                    }
                }
            }
        }
    });
    loop {
        if let Some(exit) = child.try_wait().map_err(|_| "快速词状态异常")? {
            if exit.success() {
                break;
            }
            return Err("快速词窗口启动失败；当前系统可能不兼容".into());
        }
        let d = s.data.lock().map_err(|_| "状态不可用")?;
        if !d.saved.quick_words.enabled {
            break;
        }
        let words = d.saved.quick_words.ranked(now());
        drop(d);
        let allow = status(rt).ok().is_some_and(|x| ordinary(&x));
        if writeln!(stdin, "{}", json!({"allowed":allow,"words":words})).is_err() {
            break;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    drop(stdin);
    let _ = child.kill();
    let _ = child.wait();
    Ok(())
}
pub(crate) fn powershell_cancel(
    script: &str,
    input: &Value,
    cancel: impl Fn() -> bool,
) -> Result<String, String> {
    if !cfg!(windows) {
        return Err("真实适配器仅支持Windows".into());
    }
    if cancel() {
        return Err("AI 请求已取消，本地助手继续运行".into());
    }
    let mut cmd = Process::new("powershell.exe");
    cmd.args([
        "-NoLogo",
        "-NoProfile",
        "-NonInteractive",
        "-Command",
        script,
    ])
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }
    let mut child = cmd.spawn().map_err(|_| "无法启动Windows适配器")?;
    child
        .stdin
        .take()
        .ok_or("输入管道不可用")?
        .write_all(input.to_string().as_bytes())
        .map_err(|_| "写入适配器失败")?;
    let mut stdout = child.stdout.take().ok_or("输出管道不可用")?;
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let result = stdout.by_ref().take(65537).read_to_end(&mut bytes);
        (result, bytes)
    });
    let start = Instant::now();
    let mut checked = Instant::now();
    let exit = loop {
        if checked.elapsed() > Duration::from_millis(250) {
            checked = Instant::now();
            if cancel() {
                let _ = child.kill();
                let _ = child.wait();
                let _ = reader.join();
                return Err("AI 请求已取消，本地助手继续运行".into());
            }
        }
        match child.try_wait().map_err(|_| "适配器状态不可读")? {
            Some(s) => break s,
            None if start.elapsed() > Duration::from_secs(30) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = reader.join();
                return Err("适配器超时；请重新检查设备状态".into());
            }
            None => std::thread::sleep(Duration::from_millis(20)),
        }
    };
    let (result, bytes) = reader.join().map_err(|_| "适配器读取失败")?;
    result.map_err(|_| "适配器读取失败")?;
    if cancel() {
        return Err("AI 请求已取消，本地助手继续运行".into());
    }
    if !exit.success() || bytes.len() > 65536 {
        return Err("适配器失败或响应超限；请检查设备能力/API配置".into());
    }
    String::from_utf8(bytes).map_err(|_| "适配器响应编码错误".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failing_saved_operations_never_change_live_settings_or_the_original_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("assist.json");
        let rt = Runtime::start(dir.path().join("demo.db"), true).unwrap();
        let mut assist = Assist::new(path.clone(), true).unwrap();
        handle(&assist, &rt, Request::QuickWords { enabled: true }).unwrap();
        let original = std::fs::read(&path).unwrap();
        let saved = serde_json::to_value(&assist.data.lock().unwrap().saved).unwrap();
        assist.path = path.join("blocked.json");
        for request in [
            Request::QuickWords { enabled: false },
            Request::Limit { limit: 1 },
            Request::AddWord {
                text: "祝你今天顺利。".into(),
            },
            Request::DeleteWord {
                text: "谢谢你的帮助！".into(),
            },
        ] {
            assert!(handle(&assist, &rt, request).is_err());
            assert_eq!(
                serde_json::to_value(&assist.data.lock().unwrap().saved).unwrap(),
                saved
            );
            assert_eq!(std::fs::read(&path).unwrap(), original);
        }
        assist.learn("明天再联系。");
        assert_eq!(
            serde_json::to_value(&assist.data.lock().unwrap().saved).unwrap(),
            saved
        );
        rt.shutdown();
    }

    #[test]
    fn successful_commands_persist_across_restart_and_duplicates_are_not_added() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("assist.json");
        let rt = Runtime::start(dir.path().join("demo.db"), true).unwrap();
        let assist = Assist::new(path.clone(), true).unwrap();
        handle(&assist, &rt, Request::QuickWords { enabled: true }).unwrap();
        handle(&assist, &rt, Request::Limit { limit: 30 }).unwrap();
        for _ in 0..2 {
            handle(
                &assist,
                &rt,
                Request::AddWord {
                    text: "明天再联系。".into(),
                },
            )
            .unwrap();
        }
        handle(
            &assist,
            &rt,
            Request::DeleteWord {
                text: "谢谢你的帮助！".into(),
            },
        )
        .unwrap();
        let restarted = Assist::new(path, true).unwrap();
        let state = snapshot(&restarted).unwrap();
        assert_eq!(state["enabled"], true);
        assert_eq!(state["limit"], 30);
        let words = state["words"].as_array().unwrap();
        assert_eq!(
            words.iter().filter(|w| w["text"] == "明天再联系。").count(),
            1
        );
        assert!(!words.iter().any(|w| w["text"] == "谢谢你的帮助！"));
        rt.shutdown();
    }

    #[test]
    fn brightness_changes_are_reported_even_when_learning_cannot_be_saved() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("assist.json");
        let rt = Runtime::start(dir.path().join("demo.db"), true).unwrap();
        rt.command(Command::DemoScene("game".into())).unwrap();
        let mut assist = Assist::new(path.clone(), true).unwrap();
        handle(&assist, &rt, Request::QuickWords { enabled: true }).unwrap();
        assist.path = path.join("blocked.json");
        let result = handle(
            &assist,
            &rt,
            Request::BrightnessSet {
                device: "demo:display".into(),
                before: 50,
                value: 60,
            },
        );
        assert!(result.unwrap_err().contains("亮度已调整"));
        let data = assist.data.lock().unwrap();
        assert_eq!(data.monitors[0].value, 60);
        assert!(data.saved.brightness.choices.is_empty());
        rt.shutdown();
    }
}
