#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod assist;
mod cold_ai;
mod monitors;
use assist::assist_api;
use habitos_core::{
    model::{Mode, Settings, Status},
    runtime::{Command, Runtime},
};
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager, State, WindowEvent,
};

fn run(runtime: State<'_, Runtime>, command: Command) -> Result<Status, String> {
    runtime.command(command).map_err(|e| e.to_string())
}
#[tauri::command]
fn status(runtime: State<'_, Runtime>) -> Result<Status, String> {
    run(runtime, Command::Status)
}
#[tauri::command]
fn grant_current(runtime: State<'_, Runtime>) -> Result<Status, String> {
    run(runtime, Command::GrantCurrent)
}
#[tauri::command]
fn set_permission(
    runtime: State<'_, Runtime>,
    id: String,
    observe: bool,
    mode: Mode,
) -> Result<Status, String> {
    run(runtime, Command::Permission { id, observe, mode })
}
#[tauri::command]
fn set_sensitive(
    runtime: State<'_, Runtime>,
    id: String,
    sensitive: bool,
) -> Result<Status, String> {
    run(runtime, Command::Sensitive { id, sensitive })
}
#[tauri::command]
fn capture(runtime: State<'_, Runtime>) -> Result<Status, String> {
    run(runtime, Command::Capture)
}
#[tauri::command]
fn accept(runtime: State<'_, Runtime>, id: String) -> Result<Status, String> {
    run(runtime, Command::Accept(id))
}
#[tauri::command]
fn reject(runtime: State<'_, Runtime>, id: String) -> Result<Status, String> {
    run(runtime, Command::Reject(id))
}
#[tauri::command]
fn undo(runtime: State<'_, Runtime>, id: String) -> Result<Status, String> {
    run(runtime, Command::Undo(id))
}
#[tauri::command]
fn save_settings(runtime: State<'_, Runtime>, settings: Settings) -> Result<Status, String> {
    run(runtime, Command::Settings(settings))
}
#[tauri::command]
fn forget(runtime: State<'_, Runtime>) -> Result<Status, String> {
    run(runtime, Command::Forget)
}
#[tauri::command]
fn demo_scene(runtime: State<'_, Runtime>, scene: String) -> Result<Status, String> {
    run(runtime, Command::DemoScene(scene))
}
#[tauri::command]
fn demo_volume(runtime: State<'_, Runtime>, volume: f64) -> Result<Status, String> {
    run(runtime, Command::DemoVolume(volume))
}
#[tauri::command]
fn demo_teach(runtime: State<'_, Runtime>) -> Result<Status, String> {
    run(runtime, Command::DemoTeach)
}

#[tauri::command]
fn pin_memory(runtime: State<'_, Runtime>, id: String, pinned: bool) -> Result<Status, String> {
    run(runtime, Command::PinMemory { id, pinned })
}
#[tauri::command]
fn delete_memory(runtime: State<'_, Runtime>, id: String) -> Result<Status, String> {
    run(runtime, Command::DeleteMemory(id))
}
#[tauri::command]
fn set_target(runtime: State<'_, Runtime>, id: String, enabled: bool) -> Result<Status, String> {
    run(runtime, Command::TargetEnabled { id, enabled })
}
#[tauri::command]
fn launch_app(runtime: State<'_, Runtime>, id: String) -> Result<Status, String> {
    run(runtime, Command::LaunchApp(id))
}
#[tauri::command]
fn dismiss_app(runtime: State<'_, Runtime>, id: String) -> Result<Status, String> {
    run(runtime, Command::DismissApp(id))
}
#[tauri::command]
fn demo_recommend(runtime: State<'_, Runtime>) -> Result<Status, String> {
    run(runtime, Command::DemoRecommend)
}
#[tauri::command]
fn add_target(window: tauri::WebviewWindow, runtime: State<'_, Runtime>) -> Result<Status, String> {
    #[cfg(windows)]
    if let Some(path) = habitos_core::platform::windows::choose_program(
        window.hwnd().map_err(|e| e.to_string())?.0 as isize,
    )
    .map_err(|e| e.to_string())?
    {
        return run(runtime, Command::RegisterTarget(path));
    }
    #[cfg(not(windows))]
    let _ = window;
    run(runtime, Command::Status)
}

#[tauri::command]
fn open_target(runtime: State<'_, Runtime>, id: String, session: String) -> Result<Status, String> {
    run(runtime, Command::OpenTarget { id, session })
}
#[tauri::command]
fn set_startup(runtime: State<'_, Runtime>, enabled: bool) -> Result<Status, String> {
    run(runtime, Command::Startup(enabled))
}
#[tauri::command]
fn demo_grid(runtime: State<'_, Runtime>) -> Result<Status, String> {
    run(runtime, Command::DemoGrid)
}
#[tauri::command]
fn open_panel(app: tauri::AppHandle, runtime: State<'_, Runtime>) -> Result<Status, String> {
    show_panel(&app);
    run(runtime, Command::Status)
}
#[tauri::command]
fn hide_popup(window: tauri::WebviewWindow, runtime: State<'_, Runtime>) -> Result<Status, String> {
    if window.label() == "tray" || window.label() == "recommendation" {
        window.hide().map_err(|e| e.to_string())?;
    }
    run(runtime, Command::Status)
}

#[tauri::command]
fn add_website(runtime: State<'_, Runtime>, url: String) -> Result<Status, String> {
    run(runtime, Command::RegisterWebsite(url))
}
#[tauri::command]
fn demo_websites(runtime: State<'_, Runtime>) -> Result<Status, String> {
    run(runtime, Command::DemoWebsites)
}
fn main() {
    // Browser native messaging starts this lightweight mode with the extension origin.
    // Do not create Tauri, WebViews or a second SQLite worker in this mode.
    #[cfg(windows)]
    if std::env::args()
        .nth(1)
        .is_some_and(|a| a.starts_with("chrome-extension://"))
    {
        let _ = habitos_core::browser_bridge::native_host();
        return;
    }

    let demo = std::env::args().any(|a| a == "--demo") || !cfg!(windows);
    let background = std::env::args().any(|a| a == "--background");
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .setup(move |app| {
            let directory = app.path().app_data_dir()?;
            std::fs::create_dir_all(&directory)?;
            let runtime = Runtime::start(
                directory.join(if demo {
                    "demo.sqlite3"
                } else {
                    "habitos.sqlite3"
                }),
                demo,
            )?;
            let assistant = Arc::new(
                assist::Assist::new(
                    directory.join(if demo {
                        "assist-demo.json"
                    } else {
                        "assist.json"
                    }),
                    demo,
                )
                .map_err(std::io::Error::other)?,
            );
            assistant.start(runtime.clone());
            assert!(app.manage(runtime), "Runtime already registered");
            app.manage(assistant);
            // Configured WebViews can invoke IPC as soon as they load. Tauri creates
            // automatic windows before this setup hook, so create them only after
            // the worker has started and its managed state is available.
            for config in app.config().app.windows.clone() {
                tauri::WebviewWindowBuilder::from_config(app, &config)?.build()?;
            }
            #[cfg(windows)]
            if !demo {
                let state = app.state::<Runtime>().command(Command::Status)?;
                habitos_core::browser_bridge::write_status_policy(&directory, &state)?;
                let copy = app.state::<Runtime>().inner().clone();
                let watcher = habitos_core::browser_bridge::Watcher::start(
                    directory.clone(),
                    Arc::new(move |visit| {
                        let _ = copy.command(Command::BrowserVisit(visit));
                    }),
                )?;
                app.manage(watcher);
            }
            let open = MenuItem::with_id(app, "open", "打开面板", true, None::<&str>)?;
            let pause = MenuItem::with_id(app, "pause", "暂停 / 恢复学习", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "退出 Satori", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open, &pause, &quit])?;
            TrayIconBuilder::with_id("habitos")
                .icon(app.default_window_icon().ok_or("缺少托盘图标")?.clone())
                .tooltip(if demo {
                    "Satori · 演示后台"
                } else {
                    "Satori · 事件触发，后台运行"
                })
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "open" => show_panel(app),
                    "pause" => {
                        let runtime = app.state::<Runtime>();
                        if let Ok(state) = runtime.command(Command::Status) {
                            let mut settings = state.settings;
                            settings.paused = !settings.paused;
                            if let Err(error) = runtime.command(Command::Settings(settings)) {
                                eprintln!("{error}");
                                show_panel(app);
                            }
                        }
                    }
                    "quit" => {
                        if let Ok(path) = app.path().app_data_dir() {
                            let _ = habitos_core::browser_bridge::write_policy(&path, false);
                        }
                        app.state::<Runtime>().shutdown();
                        app.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| match event {
                    TrayIconEvent::Click {
                        position,
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } => show_tray(tray.app_handle(), position),
                    TrayIconEvent::DoubleClick {
                        button: MouseButton::Left,
                        ..
                    } => show_panel(tray.app_handle()),
                    _ => {}
                })
                .build(app)?;
            let handle = app.handle().clone();
            let offered = Arc::new(Mutex::new(HashSet::<String>::new()));
            let asked = Arc::new(Mutex::new(HashSet::<String>::new()));
            app.state::<Runtime>().on_change(Arc::new(move |state| {
                if !state.demo {
                    let _ = habitos_core::browser_bridge::write_status_policy(&directory, &state);
                }

                let _ = handle.emit("habitos:status", &state);
                let recommendation = state
                    .app_recommendation
                    .as_ref()
                    .filter(|_| !state.settings.paused && state.settings.floating_cards);
                let first = recommendation.is_some_and(|p| {
                    offered
                        .lock()
                        .is_ok_and(|mut seen| seen.insert(p.id.clone()))
                });
                let learning_question = state
                    .experience
                    .question
                    .as_ref()
                    .filter(|_| !state.settings.paused && state.settings.floating_cards);
                let learning_first = learning_question.is_some_and(|e| {
                    offered.lock().is_ok_and(|mut seen| {
                        seen.insert(format!("learning:{}:{}", e.id, e.revision))
                    })
                });
                let first = first || learning_first;
                let present = recommendation.is_some() || learning_question.is_some();
                let app = handle.clone();
                let _ = handle.run_on_main_thread(move || {
                    if let Some(card) = app.get_webview_window("recommendation") {
                        if first {
                            if let Ok(Some(monitor)) = card.primary_monitor() {
                                let scale = monitor.scale_factor();
                                let area = monitor.work_area();
                                let size = tauri::PhysicalSize::new(
                                    (480.0 * scale) as u32,
                                    (350.0 * scale) as u32,
                                );
                                let _ = card.set_size(size);
                                let _ = card.set_position(tauri::PhysicalPosition::new(
                                    area.position.x + area.size.width as i32
                                        - size.width as i32
                                        - (20.0 * scale) as i32,
                                    area.position.y + area.size.height as i32
                                        - size.height as i32
                                        - (20.0 * scale) as i32,
                                ));
                            }
                            #[cfg(windows)]
                            if let Ok(hwnd) = card.hwnd() {
                                let _ = habitos_core::platform::windows::show_card_inactive(
                                    hwnd.0 as isize,
                                );
                            }
                            #[cfg(not(windows))]
                            let _ = card.show();
                        } else if !present {
                            let _ = card.hide();
                        }
                    }
                });

                if let Some(current) = &state.current {
                    if state
                        .apps
                        .iter()
                        .any(|p| p.id == current.app_id && p.sensitive && !p.observe && !p.explicit)
                        && asked
                            .lock()
                            .is_ok_and(|mut seen| seen.insert(current.app_id.clone()))
                    {
                        let app = handle.clone();
                        let _ = handle.run_on_main_thread(move || show_panel(&app));
                    }
                }
            }))?;
            if !background {
                show_panel(app.handle());
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if matches!(event, WindowEvent::Focused(false)) && window.label() == "tray" {
                let _ = window.hide();
            }
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
                if window.label() == "recommendation" {
                    let runtime = window.state::<Runtime>();
                    if let Ok(state) = runtime.command(Command::Status) {
                        if let Some(p) = state.app_recommendation {
                            let _ = runtime.command(Command::DismissApp(p.id));
                        }
                    }
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            assist_api,
            status,
            add_website,
            demo_websites,
            grant_current,
            set_permission,
            set_sensitive,
            capture,
            accept,
            reject,
            undo,
            save_settings,
            forget,
            demo_scene,
            demo_volume,
            demo_teach,
            pin_memory,
            delete_memory,
            set_target,
            add_target,
            launch_app,
            dismiss_app,
            demo_recommend,
            open_target,
            set_startup,
            demo_grid,
            open_panel,
            hide_popup
        ])
        .run(app_context())
        .expect("Satori 启动失败，请从终端运行以查看错误");
}
fn show_tray(app: &tauri::AppHandle, position: tauri::PhysicalPosition<f64>) {
    let Some(window) = app.get_webview_window("tray") else {
        return;
    };
    if window.is_visible().unwrap_or(false) {
        let _ = window.hide();
        return;
    }
    // Refresh ranking on explicit opening; uses one native snapshot, no periodic polling.
    let _ = app.state::<Runtime>().command(Command::Refresh);
    let monitors = window.available_monitors().unwrap_or_default();
    let monitor = monitors
        .iter()
        .find(|m| {
            let origin = m.position();
            let size = m.size();
            position.x >= origin.x as f64
                && position.y >= origin.y as f64
                && position.x < origin.x as f64 + size.width as f64
                && position.y < origin.y as f64 + size.height as f64
        })
        .or_else(|| monitors.first());
    if let Some(monitor) = monitor {
        let scale = monitor.scale_factor();
        let area = monitor.work_area();
        let width = (480.0 * scale) as u32;
        let height = (350.0 * scale) as u32;
        let gap = (12.0 * scale) as i32;
        let x = (position.x as i32 - width as i32 / 2).clamp(
            area.position.x,
            area.position.x + (area.size.width as i32 - width as i32).max(0),
        );
        let y = (position.y as i32 - height as i32 - gap).clamp(
            area.position.y,
            area.position.y + (area.size.height as i32 - height as i32).max(0),
        );
        let _ = window.set_size(tauri::PhysicalSize::new(width, height));
        let _ = window.set_position(tauri::PhysicalPosition::new(x, y));
    }
    let _ = window.show();
    let _ = window.set_focus();
}

fn show_panel(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn app_context() -> tauri::Context<tauri::Wry> {
    let mut context = tauri::generate_context!();
    if !tauri::is_dev() {
        assert!(
            context.assets().get(&"index.html".into()).is_some(),
            "Production frontend is missing from the executable"
        );
        // Production always loads bundled assets, with no development-server fallback.
        context.config_mut().build.dev_url = None;
    }
    context
}

#[cfg(all(test, not(dev)))]
mod packaging_tests {
    #[test]
    fn production_context_contains_html_and_all_frontend_dependencies() {
        let context = super::app_context();
        assert!(!tauri::is_dev());
        assert!(context.config().build.dev_url.is_none());
        assert_eq!(context.config().app.windows.len(), 3);
        assert!(
            context
                .config()
                .app
                .windows
                .iter()
                .all(|window| !window.create),
            "Automatic WebViews can invoke status before setup registers Runtime"
        );
        let assets = context.assets();
        let html = assets.get(&"index.html".into()).expect("bundled HTML");
        let html = std::str::from_utf8(&html).unwrap();
        assert!(!html.contains("127.0.0.1") && !html.contains("@vite/client"));
        let mut dependencies = 0;
        for attribute in ["src=\"", "href=\""] {
            for part in html.split(attribute).skip(1) {
                let path = part.split('"').next().unwrap();
                if path.starts_with("/assets/") {
                    assert!(
                        assets.get(&path.into()).is_some(),
                        "Missing bundled asset {path}"
                    );
                    dependencies += 1;
                }
            }
        }
        assert!(
            dependencies >= 2,
            "HTML must reference bundled JavaScript and CSS"
        );
    }
}
