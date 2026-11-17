// No console window in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod autostart;
mod checker;
mod core;
mod launch;
mod logbuf;
mod paths;
mod scale;
mod settings;
mod sites;
mod tgws;
mod tray;
mod updater;
mod zapret;

use std::sync::atomic::Ordering;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, RunEvent, State, WindowEvent};

use crate::core::{Core, StateDto, UpdateOutcome};
use crate::logbuf::{LogBuf, LogEntry};
use crate::settings::Settings;

type CoreState<'a> = State<'a, Arc<Core>>;

pub fn core_of(app: &AppHandle) -> Option<Arc<Core>> {
    app.try_state::<Arc<Core>>().map(|s| s.inner().clone())
}

#[tauri::command]
fn get_state(core: CoreState) -> StateDto {
    core.state()
}

#[tauri::command]
fn start_pick(core: CoreState) {
    let c = core.inner().clone();
    tauri::async_runtime::spawn(async move { c.pick().await });
}

#[tauri::command]
fn cancel_pick(core: CoreState) {
    core.cancel.store(true, Ordering::SeqCst);
}

#[tauri::command]
async fn set_enabled(core: CoreState<'_>, on: bool) -> Result<StateDto, String> {
    Ok(core.set_enabled(on))
}

#[tauri::command]
async fn set_strategy(core: CoreState<'_>, name: String) -> Result<StateDto, String> {
    let known = core.lock().strategies.iter().any(|s| s.name == name);
    if !known {
        return Err("нет такой стратегии".into());
    }
    core.update_settings(|s| s.strategy = Some(name.clone()));
    core.log(format!("Стратегия: {name}"));
    let enabled = core.settings().enabled;
    if enabled {
        core.start_current();
    }
    Ok(core.emit_state())
}

#[tauri::command]
async fn set_setting(core: CoreState<'_>, key: String, value: bool) -> Result<StateDto, String> {
    match key.as_str() {
        "autoUpdate" => core.update_settings(|s| s.auto_update = value),
        "closeToTray" => core.update_settings(|s| s.close_to_tray = value),
        "sounds" => core.update_settings(|s| s.sounds = value),
        "intro" => core.update_settings(|s| s.intro = value),
        "telegram" => return Ok(core.set_telegram(value)),
        "gameMode" => {
            core.update_settings(|s| s.game_mode = value);
            core.log(if value { "Игровой режим включён" } else { "Игровой режим выключен" });
            core.restart_if_running();
        }
        "autostart" => {
            let r = if value { autostart::enable() } else { autostart::disable() };
            if let Err(e) = r {
                core.log(format!("Автозапуск: {e}"));
                return Err(e);
            }
            core.update_settings(|s| s.autostart = value);
        }
        _ => return Err("неизвестная настройка".into()),
    }
    Ok(core.emit_state())
}

/// zapret and tg-ws-proxy: "updated:<what>" when something was installed, "latest" otherwise.
#[tauri::command]
async fn check_updates(core: CoreState<'_>) -> Result<String, String> {
    let z = core.inner().clone().update().await;
    let t = core.inner().clone().tg_update().await;
    let mut updated = Vec::new();
    if let Ok(UpdateOutcome::Updated(v)) = &z {
        updated.push(format!("zapret {v}"));
    }
    if let Ok(UpdateOutcome::Updated(v)) = &t {
        updated.push(format!("tg-ws-proxy {v}"));
    }
    if let Err(e) = &z {
        core.log(format!("Проверка обновлений zapret: {e}"));
    }
    if let Err(e) = &t {
        core.log(format!("Проверка обновлений tg-ws-proxy: {e}"));
    }
    match core.app_check().await {
        Ok(Some(v)) => core.log(format!("Доступна новая версия kidpi {v}")),
        Ok(None) => {}
        Err(e) => core.log(format!("Проверка обновлений kidpi: {e}")),
    }
    if !updated.is_empty() {
        Ok(format!("updated:{}", updated.join(", ")))
    } else if z.is_err() && t.is_err() {
        Err(z.err().unwrap_or_default())
    } else {
        Ok("latest".into())
    }
}

/// Links from the window (the author's Telegram). Opened as the signed-in user, so
/// the browser does not inherit kidpi's admin rights.
#[tauri::command]
async fn open_url(url: String) -> Result<(), String> {
    if !url.starts_with("https://") || url.contains(['"', ' ', '\n']) {
        return Err("ссылка не подходит".into());
    }
    tauri::async_runtime::spawn_blocking(move || launch::open_url(&url)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
async fn install_app_update(core: CoreState<'_>) -> Result<(), String> {
    let r = core.app_install().await;
    if let Err(e) = &r {
        core.log(format!("Обновление kidpi: {e}"));
    }
    r
}

/// "link" | "exe" — Telegram got the link; "unknown" — no Telegram Desktop was found,
/// Windows was asked to open the link anyway.
#[tauri::command]
async fn open_tg_link(core: CoreState<'_>) -> Result<String, String> {
    let link = core.state().tg_link.ok_or("ссылка появится после включения")?;
    let r = tauri::async_runtime::spawn_blocking(move || launch::open_telegram(&link))
        .await
        .map_err(|e| e.to_string())?;
    match r {
        Ok(launch::TgOpen::Link) => Ok("link".into()),
        Ok(launch::TgOpen::Exe) => {
            core.log("Ссылка на прокси передана Telegram Desktop напрямую: tg:// не зарегистрирован");
            Ok("exe".into())
        }
        Ok(launch::TgOpen::Unknown) => {
            core.log("Telegram Desktop не найден: ссылку на прокси открывает Windows");
            Ok("unknown".into())
        }
        Err(e) => {
            core.log(format!("Не удалось открыть Telegram: {e}"));
            Err("open".into())
        }
    }
}

fn site_op(core: &Core, input: &str, add: bool) -> Result<String, String> {
    let dir = core.lock().install.as_ref().map(|i| i.dir.clone()).ok_or("zapret не установлен")?;
    let r = if add { sites::add(&dir, input) } else { sites::remove(&dir, input) };
    match r {
        Ok(d) => {
            core.log(if add { format!("Сайт добавлен в обход: {d}") } else { format!("Сайт убран из обхода: {d}") });
            core.restart_if_running();
            Ok(d)
        }
        Err(e) => Err(e.code()),
    }
}

#[tauri::command]
async fn add_site(core: CoreState<'_>, input: String) -> Result<String, String> {
    site_op(&core, &input, true)
}

#[tauri::command]
async fn remove_site(core: CoreState<'_>, input: String) -> Result<String, String> {
    site_op(&core, &input, false)
}

#[tauri::command]
fn get_log(core: CoreState) -> Vec<LogEntry> {
    core.log.list()
}

#[tauri::command]
fn open_log_folder() {
    let dir = paths::logs_dir();
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::process::Command::new("explorer").arg(dir).spawn();
}

/// "Масштаб" in settings. Returns the scale actually applied.
#[tauri::command]
fn set_scale(app: AppHandle, core: CoreState, scale: f64) -> f64 {
    let new = scale::snap(scale).min(scale::max(&app));
    let old = core.settings().scale;
    if (new - old).abs() > f64::EPSILON {
        core.update_settings(|s| s.scale = new);
        scale::change(&app, old, new);
        core.emit_state();
    }
    new
}

/// The largest scale the screen the window is on has room for.
#[tauri::command]
fn scale_max(app: AppHandle) -> f64 {
    scale::max(&app)
}

/// The main window's page asks once whether to play the launch intro.
#[tauri::command]
fn take_intro(core: CoreState) -> bool {
    core.intro_pending.swap(false, Ordering::SeqCst)
}

/// Each window's page asks for its zoom as it loads.
#[tauri::command]
fn sync_zoom(window: tauri::WebviewWindow, core: CoreState) {
    scale::zoom(&window, core.settings().scale);
}

#[tauri::command]
fn mark_tray_notified(core: CoreState) {
    core.update_settings(|s| s.tray_notified = true);
}

#[tauri::command]
fn hide_main(app: AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.hide();
    }
}

#[tauri::command]
fn minimize_main(app: AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.minimize();
    }
}

#[tauri::command]
fn show_main(app: AppHandle) {
    tray::show_main(&app);
}

#[tauri::command]
fn quit_app(app: AppHandle, core: CoreState) {
    core.stop_all();
    app.exit(0);
}

/// Buttons of the tray menu window.
#[tauri::command]
async fn tray_action(app: AppHandle, core: CoreState<'_>, action: String) -> Result<(), String> {
    match action.as_str() {
        "toggle" => {
            let on = core.state().phase == "on";
            if !on && core.settings().strategy.is_none() {
                tray::show_main(&app);
            } else {
                core.set_enabled(!on);
            }
        }
        "open" => tray::show_main(&app),
        "repick" => {
            tray::hide_menu(&app);
            tray::show_main(&app);
            let c = core.inner().clone();
            tauri::async_runtime::spawn(async move { c.pick().await });
        }
        "settings" => {
            tray::show_main(&app);
            let _ = app.emit_to("main", "open-settings", ());
        }
        "quit" => {
            core.stop_all();
            app.exit(0);
        }
        _ => {}
    }
    Ok(())
}

fn main() {
    let autostart_launch = std::env::args().any(|a| a == "--autostart");

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            tray::show_main(app);
        }))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            get_state,
            start_pick,
            cancel_pick,
            set_enabled,
            set_strategy,
            set_setting,
            check_updates,
            open_tg_link,
            open_url,
            install_app_update,
            add_site,
            remove_site,
            get_log,
            open_log_folder,
            mark_tray_notified,
            set_scale,
            scale_max,
            sync_zoom,
            take_intro,
            hide_main,
            minimize_main,
            show_main,
            quit_app,
            tray_action
        ])
        .on_window_event(|window, event| match (window.label(), event) {
            ("main", WindowEvent::CloseRequested { api, .. }) => {
                // The window decides: first time it explains the tray, then hides.
                api.prevent_close();
                let _ = window.emit("close-requested", ());
            }
            ("traymenu", WindowEvent::CloseRequested { api, .. }) => {
                api.prevent_close();
                let _ = window.hide();
            }
            ("traymenu", WindowEvent::Focused(false)) => {
                let _ = window.hide();
            }
            _ => {}
        })
        .setup(move |app| {
            let handle = app.handle().clone();
            let log = LogBuf::new();
            let mut settings = Settings::load();

            let res_dir = app.path().resource_dir().unwrap_or_default();
            let bundled = res_dir.join("zapret");
            let tg_bundled = res_dir.join("tgws");

            // Anything a killed or crashed kidpi left running would hold its files
            // and ports. There is only ever one kidpi (single instance), so whatever
            // runs from kidpi's folders now is a leftover.
            let leftovers = zapret::kill_under(&paths::base_dir())
                + zapret::kill_under(&bundled)
                + tgws::kill_leftovers(&tg_bundled);
            if leftovers > 0 {
                std::thread::sleep(std::time::Duration::from_millis(400));
            }

            let mut startup_error: Option<String> = None;
            let install = match zapret::ensure_installed(&bundled, settings.zapret_version.as_deref()) {
                Ok((i, warning)) => {
                    if let Some(w) = warning {
                        log.push(&handle, w);
                    }
                    Some(i)
                }
                Err(e) => {
                    log.push(&handle, format!("Ошибка: {e}"));
                    startup_error = Some(e);
                    None
                }
            };
            let strategies = install.as_ref().map(|i| zapret::list_strategies(&i.dir)).unwrap_or_default();
            if let Some(i) = &install {
                settings.zapret_version = zapret::folder_key(i);
                if strategies.is_empty() {
                    let e = format!("В папке {} нет стратегий zapret — переустановите kidpi", i.dir.display());
                    log.push(&handle, format!("Ошибка: {e}"));
                    startup_error = Some(e);
                }
            }
            if settings.strategy.as_ref().map_or(false, |n| !strategies.iter().any(|s| &s.name == n)) {
                settings.strategy = None;
            }
            let tg = match tgws::ensure_installed(&tg_bundled, settings.tg_version.as_deref()) {
                Ok(t) => {
                    settings.tg_version = tgws::folder_key(&t);
                    Some(t)
                }
                Err(e) => {
                    log.push(&handle, format!("Telegram: {e}"));
                    None
                }
            };
            settings.save();
            let ui_scale = settings.scale;
            let play_intro = settings.intro && !autostart_launch;

            let core = Arc::new(Core::new(handle.clone(), settings, install, strategies, tg, log));
            if let Some(e) = startup_error {
                core.lock().error = Some(e);
            }
            core.intro_pending.store(play_intro, Ordering::SeqCst);
            app.manage(core.clone());
            tray::build(&handle)?;
            scale::init(&handle, ui_scale);

            if !autostart_launch {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.show();
                    let _ = w.set_focus();
                }
            }

            let c = core.clone();
            tauri::async_runtime::spawn(async move {
                c.log(if autostart_launch { "kidpi запущен вместе с Windows" } else { "kidpi запущен" });
                let s = c.settings();
                // other people's winws.exe only goes when kidpi's bypass is about to start
                let bypass = s.enabled && s.strategy.is_some();
                if bypass {
                    let _ = tokio::task::spawn_blocking(zapret::kill_stray).await;
                    if c.start_current() {
                        c.log(format!("Обход включён: {}", s.strategy.unwrap_or_default()));
                    }
                }
                if s.telegram && c.tg_start() {
                    c.log("Ускорение Telegram включено");
                }
                c.emit_state();
                // keep the logon task in line with the setting
                let want = s.autostart;
                let res = tokio::task::spawn_blocking(move || {
                    if want && !autostart::is_enabled() {
                        autostart::enable()
                    } else {
                        Ok(())
                    }
                })
                .await;
                if let Ok(Err(e)) = res {
                    c.log(format!("Автозапуск: {e}"));
                }
            });
            tauri::async_runtime::spawn(core.clone().watchdog());
            tauri::async_runtime::spawn(core.clone().auto_update_loop());
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("failed to build kidpi");

    app.run(|app, event| {
        if let RunEvent::Exit = event {
            if let Some(core) = core_of(app) {
                core.stop_all();
            }
        }
    });
}
