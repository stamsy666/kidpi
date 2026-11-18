//! kidpi's state and everything that changes it: running a strategy, picking
//! one, updating zapret, and keeping an eye on the bypass in the background.

use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::UpdaterExt;

use crate::checker::{self, Score};
use crate::logbuf::LogBuf;
use crate::settings::Settings;
use crate::zapret::{self, Install, Runner, Strategy};
use crate::{tgws, tray, updater};

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub index: usize,
    pub total: usize,
    pub name: String,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct StateDto {
    /// "pick" (no strategy yet) | "picking" | "on" | "off"
    pub phase: &'static str,
    pub strategy: Option<String>,
    pub strategies: Vec<String>,
    pub progress: Option<Progress>,
    pub pick_failed: bool,
    pub zapret_version: String,
    pub app_version: String,
    pub auto_update: bool,
    pub autostart: bool,
    pub close_to_tray: bool,
    pub game_mode: bool,
    pub sounds: bool,
    pub tray_notified: bool,
    pub error: Option<String>,
    /// "Ускорение Telegram" switch
    pub telegram: bool,
    pub tg_running: bool,
    pub tg_version: String,
    pub tg_link: Option<String>,
    pub tg_error: Option<String>,
    /// A newer kidpi found on GitHub, ready to install.
    pub app_update: Option<String>,
    /// Interface scale, 1.0 = 100 %.
    pub scale: f64,
    pub intro: bool,
}

pub struct Inner {
    pub settings: Settings,
    pub install: Option<Install>,
    pub strategies: Vec<Strategy>,
    pub runner: Runner,
    pub progress: Option<Progress>,
    pub pick_failed: bool,
    pub error: Option<String>,
    pub tg: Option<tgws::Install>,
    pub tg_runner: tgws::Runner,
    pub tg_error: Option<String>,
    pub app_update: Option<String>,
}

pub struct Core {
    pub app: AppHandle,
    pub inner: Mutex<Inner>,
    pub log: LogBuf,
    pub cancel: AtomicBool,
    /// Picking, updating and the watchdog's checks never overlap.
    pub op: tokio::sync::Mutex<()>,
    last_on: AtomicBool,
    /// The window opened at launch and the intro has not played yet.
    pub intro_pending: AtomicBool,
}

pub enum UpdateOutcome {
    Latest(String),
    Updated(String),
}

impl Core {
    pub fn new(
        app: AppHandle,
        settings: Settings,
        install: Option<Install>,
        strategies: Vec<Strategy>,
        tg: Option<tgws::Install>,
        log: LogBuf,
    ) -> Self {
        Self {
            app,
            inner: Mutex::new(Inner {
                settings,
                install,
                strategies,
                runner: Runner::new(),
                progress: None,
                pick_failed: false,
                error: None,
                tg,
                tg_runner: tgws::Runner::new(),
                tg_error: None,
                app_update: None,
            }),
            log,
            cancel: AtomicBool::new(false),
            op: tokio::sync::Mutex::new(()),
            last_on: AtomicBool::new(false),
            intro_pending: AtomicBool::new(false),
        }
    }

    pub fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub fn log(&self, text: impl Into<String>) {
        self.log.push(&self.app, text);
    }

    pub fn state(&self) -> StateDto {
        let mut g = self.lock();
        let running = g.runner.is_running();
        let phase = if g.progress.is_some() {
            "picking"
        } else if g.settings.strategy.is_none() {
            "pick"
        } else if running {
            "on"
        } else {
            "off"
        };
        StateDto {
            phase,
            strategy: g.settings.strategy.clone(),
            strategies: g.strategies.iter().map(|s| s.name.clone()).collect(),
            progress: g.progress.clone(),
            pick_failed: g.pick_failed,
            zapret_version: g.install.as_ref().map(|i| i.version.clone()).unwrap_or_else(|| "—".into()),
            app_version: self.app.package_info().version.to_string(),
            auto_update: g.settings.auto_update,
            autostart: g.settings.autostart,
            close_to_tray: g.settings.close_to_tray,
            game_mode: g.settings.game_mode,
            sounds: g.settings.sounds,
            tray_notified: g.settings.tray_notified,
            error: g.error.clone(),
            telegram: g.settings.telegram,
            tg_running: g.tg_runner.is_running(),
            tg_version: g.tg.as_ref().map(|t| t.version.clone()).unwrap_or_else(|| "—".into()),
            tg_link: g.settings.tg_secret.as_ref().map(|s| tgws::link(g.settings.tg_port, s)),
            tg_error: g.tg_error.clone(),
            app_update: g.app_update.clone(),
            scale: g.settings.scale,
            intro: g.settings.intro,
        }
    }

    pub fn emit_state(&self) -> StateDto {
        let s = self.state();
        let on = s.phase == "on";
        if self.last_on.swap(on, Ordering::SeqCst) != on {
            tray::set_on(&self.app, on);
        }
        let _ = self.app.emit("state", &s);
        s
    }

    pub fn settings(&self) -> Settings {
        self.lock().settings.clone()
    }

    pub fn update_settings(&self, f: impl FnOnce(&mut Settings)) {
        let mut g = self.lock();
        f(&mut g.settings);
        g.settings.save();
    }

    /// Starts the saved strategy. Errors go to the journal and the state.
    pub fn start_current(&self) -> bool {
        let mut g = self.lock();
        let Some(install) = g.install.clone() else {
            g.error = Some("zapret не установлен".into());
            return false;
        };
        let Some(name) = g.settings.strategy.clone() else { return false };
        let Some(s) = g.strategies.iter().find(|s| s.name == name).cloned() else {
            g.settings.strategy = None;
            g.settings.save();
            return false;
        };
        let game = g.settings.game_mode;
        match g.runner.start(&install, &s, game) {
            Ok(()) => {
                g.error = None;
                drop(g);
                true
            }
            Err(e) => {
                g.error = Some(e.clone());
                drop(g);
                self.log(format!("Не удалось включить обход: {e}"));
                false
            }
        }
    }

    pub fn stop(&self) {
        self.lock().runner.stop();
    }

    /// Everything kidpi started, before it exits.
    pub fn stop_all(&self) {
        let mut g = self.lock();
        g.runner.stop();
        g.tg_runner.stop();
    }

    // ---------- kidpi itself (GitHub releases) ----------

    fn app_updater(&self) -> Result<tauri_plugin_updater::Updater, String> {
        let repo = self.settings().update_repo;
        let url = format!("https://github.com/{}/releases/latest/download/latest.json", repo.trim().trim_matches('/'));
        let url: tauri::Url = url.parse().map_err(|_| "неверное имя репозитория обновлений".to_string())?;
        self.app
            .updater_builder()
            .endpoints(vec![url])
            .map_err(|e| e.to_string())?
            .build()
            .map_err(|e| e.to_string())
    }

    /// Looks for a newer kidpi; remembers it so Settings can offer to install it.
    pub async fn app_check(&self) -> Result<Option<String>, String> {
        let found = self.app_updater()?.check().await.map_err(|e| format!("не удалось проверить: {e}"))?;
        let v = found.map(|u| u.version);
        self.lock().app_update = v.clone();
        self.emit_state();
        Ok(v)
    }

    /// Downloads the signed installer, stops the bypass and hands over to the
    /// installer, which starts kidpi again when it is done.
    pub async fn app_install(&self) -> Result<(), String> {
        let Some(u) = self.app_updater()?.check().await.map_err(|e| format!("не удалось проверить: {e}"))? else {
            self.lock().app_update = None;
            self.emit_state();
            return Err("новой версии нет".into());
        };
        self.log(format!("Скачиваю kidpi {}", u.version));
        let bytes = u.download(|_, _| {}, || {}).await.map_err(|e| format!("не удалось скачать обновление: {e}"))?;
        self.log(format!("Устанавливаю kidpi {} — программа перезапустится", u.version));
        self.stop_all();
        u.install(bytes).map_err(|e| format!("не удалось установить обновление: {e}"))?;
        Ok(())
    }

    // ---------- Telegram (tg-ws-proxy) ----------

    pub fn tg_start(&self) -> bool {
        let mut g = self.lock();
        let Some(inst) = g.tg.clone() else {
            g.tg_error = Some("tg-ws-proxy не установлен".into());
            return false;
        };
        let port = g.settings.tg_port;
        let secret = match g.settings.tg_secret.clone() {
            Some(s) if s.len() == 32 => s,
            _ => {
                let s = tgws::new_secret();
                g.settings.tg_secret = Some(s.clone());
                g.settings.save();
                s
            }
        };
        match g.tg_runner.start(&inst, port, &secret) {
            Ok(()) => {
                g.tg_error = None;
                true
            }
            Err(e) => {
                g.tg_error = Some(e.clone());
                drop(g);
                self.log(format!("Telegram: {e}"));
                false
            }
        }
    }

    pub fn set_telegram(&self, on: bool) -> StateDto {
        self.update_settings(|s| s.telegram = on);
        if on {
            if self.tg_start() {
                self.log("Ускорение Telegram включено");
                self.tg_check_soon();
            }
        } else {
            let mut g = self.lock();
            g.tg_runner.stop();
            g.tg_error = None;
            drop(g);
            self.log("Ускорение Telegram выключено");
        }
        self.emit_state()
    }

    /// The proxy should be listening a few seconds after the start.
    fn tg_check_soon(&self) {
        let app = self.app.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(Duration::from_millis(3500)).await;
            if let Some(core) = crate::core_of(&app) {
                let (want, alive, port) = {
                    let mut g = core.lock();
                    (g.settings.telegram, g.tg_runner.is_running(), g.settings.tg_port)
                };
                if want && !alive {
                    let tail = tgws::console_tail();
                    core.log(format!("tg-ws-proxy закрылся сразу после запуска: {tail}"));
                    core.lock().tg_error = Some("tg-ws-proxy не запустился. Подробности в журнале".into());
                } else if want && !tgws::listening(port) {
                    core.log(format!("tg-ws-proxy запущен, но порт {port} пока не отвечает"));
                }
                core.emit_state();
            }
        });
    }

    /// Updates tg-ws-proxy from its GitHub releases; goes back to the old
    /// version if the new one does not start.
    pub async fn tg_update(self: Arc<Self>) -> Result<UpdateOutcome, String> {
        let _op = self.op.lock().await;
        let inst = self.lock().tg.clone().ok_or("tg-ws-proxy не установлен")?;
        let tag = tgws::latest_tag().await?;
        let ver = tgws::version_of(&tag);
        if !zapret::newer(&ver, &inst.version) {
            return Ok(UpdateOutcome::Latest(inst.version));
        }
        self.log(format!("Вышла новая версия tg-ws-proxy {ver}"));
        let dir = tgws::install_tag(&tag).await?;
        let new_inst = tgws::Install { runtime: inst.runtime.clone(), app: dir, version: ver.clone(), tweaks: inst.tweaks.clone() };
        let (was_on, port) = {
            let mut g = self.lock();
            g.tg = Some(new_inst.clone());
            g.settings.tg_version = tgws::folder_key(&new_inst);
            g.settings.save();
            (g.settings.telegram, g.settings.tg_port)
        };
        if was_on {
            self.tg_start();
            tokio::time::sleep(Duration::from_secs(4)).await;
            let alive = self.lock().tg_runner.is_running();
            if !alive || !tgws::listening(port) {
                let tail = tgws::console_tail();
                {
                    let mut g = self.lock();
                    g.tg = Some(inst.clone());
                    g.settings.tg_version = tgws::folder_key(&inst);
                    g.settings.save();
                }
                self.tg_start();
                self.log(format!("tg-ws-proxy {ver} не запустился ({tail}) — вернул версию {}", inst.version));
                self.emit_state();
                return Ok(UpdateOutcome::Latest(inst.version));
            }
        }
        tgws::cleanup(&[new_inst.app.as_path(), inst.app.as_path()]);
        self.log(format!("tg-ws-proxy обновлён с {} до {ver}", inst.version));
        self.emit_state();
        Ok(UpdateOutcome::Updated(ver))
    }

    /// What zapret's .bat files do before winws.exe starts: other winws.exe go,
    /// TCP timestamps go on. Blocking (taskkill, netsh).
    pub fn prepare_winws(&self) {
        zapret::kill_stray();
        if let Some(msg) = zapret::enable_tcp_timestamps() {
            self.log(msg);
        }
    }

    /// Turns the bypass on or off from the big button or the tray menu.
    pub fn set_enabled(&self, on: bool) -> StateDto {
        if on {
            self.prepare_winws();
            let ok = self.start_current();
            self.update_settings(|s| s.enabled = true);
            if ok {
                let name = self.settings().strategy.unwrap_or_default();
                self.log(format!("Обход включён: {name}"));
                self.check_soon();
            }
        } else {
            self.stop();
            self.update_settings(|s| s.enabled = false);
            self.log("Обход выключен");
        }
        self.emit_state()
    }

    /// Re-applies the strategy after its inputs changed (lists, game mode, strategy).
    pub fn restart_if_running(&self) {
        let running = self.lock().runner.is_running();
        if running {
            self.start_current();
        }
    }

    /// A few seconds after turning on, makes sure winws.exe is still alive:
    /// an antivirus killing it is the most common failure.
    fn check_soon(&self) {
        let app = self.app.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(Duration::from_millis(2500)).await;
            if let Some(core) = crate::core_of(&app) {
                let dead = {
                    let mut g = core.lock();
                    g.settings.enabled && g.settings.strategy.is_some() && g.progress.is_none() && !g.runner.is_running()
                };
                if dead {
                    core.log(format!("winws.exe закрылся сразу после запуска: {}", zapret::winws_tail()));
                    core.lock().error = Some("winws.exe закрылся. Проверьте, не удалил ли антивирус WinDivert".into());
                    core.emit_state();
                }
            }
        });
    }

    /// Tries every strategy and keeps the one under which the most Discord and
    /// YouTube endpoints answer, the fastest on a tie.
    pub async fn pick(self: Arc<Self>) {
        let _op = self.op.lock().await;
        self.cancel.store(false, Ordering::SeqCst);
        let (install, strategies, game, prev, was_enabled) = {
            let mut g = self.lock();
            g.runner.stop();
            g.pick_failed = false;
            g.error = None;
            let total = g.strategies.len();
            g.progress = Some(Progress { index: 0, total, name: g.strategies.first().map(|s| s.name.clone()).unwrap_or_default() });
            (g.install.clone(), g.strategies.clone(), g.settings.game_mode, g.settings.strategy.clone(), g.settings.enabled)
        };
        self.emit_state();

        let Some(install) = install.filter(|_| !strategies.is_empty()) else {
            {
                let mut g = self.lock();
                g.progress = None;
                g.pick_failed = true;
                if g.error.is_none() {
                    g.error = Some("zapret не установлен или в нём нет стратегий".into());
                }
            }
            self.log("Подбор невозможен: zapret не найден");
            self.emit_state();
            return;
        };

        self.log(format!("Подбор стратегии: проверяю {} вариантов", strategies.len()));
        {
            let me = self.clone();
            let _ = tokio::task::spawn_blocking(move || me.prepare_winws()).await;
        }

        let total = strategies.len();
        let mut best: Option<(usize, Score)> = None;
        let mut cancelled = false;
        for (i, s) in strategies.iter().enumerate() {
            if self.cancel.load(Ordering::SeqCst) {
                cancelled = true;
                break;
            }
            self.lock().progress = Some(Progress { index: i + 1, total, name: s.name.clone() });
            self.emit_state();

            let started = self.lock().runner.start(&install, s, game);
            if let Err(e) = started {
                self.log(format!("{}: {e}", s.name));
                continue;
            }
            // the first start also loads the WinDivert driver
            tokio::time::sleep(Duration::from_millis(if i == 0 { 1500 } else { 600 })).await;
            if !self.lock().runner.is_running() {
                self.log(format!("{}: winws.exe сразу закрылся ({})", s.name, zapret::winws_tail()));
                continue;
            }
            let score = checker::check_all(2500).await;
            self.lock().runner.stop();
            if score.ok > 0 && best.as_ref().map_or(true, |(_, b)| score.better_than(b)) {
                best = Some((i, score));
            }
        }

        self.lock().progress = None;
        if cancelled {
            self.log("Подбор отменён");
            if prev.is_some() && was_enabled {
                self.start_current();
            }
        } else if let Some((i, sc)) = best {
            let name = strategies[i].name.clone();
            self.update_settings(|s| {
                s.strategy = Some(name.clone());
                s.enabled = true;
            });
            if self.start_current() {
                self.log(format!("Подбор завершён — {name}: отвечают {} из {} адресов Discord и YouTube", sc.ok, sc.total));
                self.log(format!("Обход включён: {name}"));
            }
        } else {
            self.lock().pick_failed = true;
            self.log("Ни одна стратегия не открыла Discord и YouTube");
            if prev.is_some() && was_enabled {
                self.start_current();
            }
        }
        self.emit_state();
    }

    /// Checks GitHub for a newer zapret and switches to it. If the bypass
    /// works worse on the new version, goes back to the old one.
    pub async fn update(self: Arc<Self>) -> Result<UpdateOutcome, String> {
        let _op = self.op.lock().await;
        let (install, was_running) = {
            let mut g = self.lock();
            (g.install.clone(), g.runner.is_running())
        };
        let install = install.ok_or("zapret не установлен")?;
        let latest = updater::latest_version().await?;
        if !zapret::newer(&latest, &install.version) {
            return Ok(UpdateOutcome::Latest(install.version));
        }
        self.log(format!("На GitHub вышла новая версия zapret {latest}"));

        let baseline = if was_running { Some(checker::check_all(3000).await) } else { None };
        let dir = updater::install(&latest, &install.dir).await?;
        let new_install = Install { dir, version: latest.clone() };
        let switch_to = |core: &Core, inst: &Install| {
            let mut g = core.lock();
            g.runner.stop();
            g.strategies = zapret::list_strategies(&inst.dir);
            g.install = Some(inst.clone());
            g.settings.zapret_version = zapret::folder_key(inst);
            let keep = g.settings.strategy.as_ref().map_or(false, |n| g.strategies.iter().any(|s| &s.name == n));
            if !keep {
                g.settings.strategy = None;
            }
            g.settings.save();
            keep
        };

        let kept = switch_to(&self, &new_install);
        if was_running && kept {
            self.start_current();
        }

        if let (Some(base), true) = (baseline, was_running && kept) {
            tokio::time::sleep(Duration::from_millis(1500)).await;
            let after = checker::check_all(3000).await;
            if base.ok > 0 && after.ok < base.ok {
                switch_to(&self, &install);
                self.start_current();
                self.log(format!("С zapret {latest} обход работает хуже — вернул версию {}", install.version));
                self.emit_state();
                return Ok(UpdateOutcome::Latest(install.version));
            }
        }

        updater::cleanup(&[new_install.dir.as_path(), install.dir.as_path()]);
        self.log(format!("zapret обновлён с {} до {latest}, списки сайтов сохранены", install.version));
        self.emit_state();
        if was_running && !kept {
            self.log("Прежней стратегии нет в новой версии — подбираю заново");
            let me = self.clone();
            drop(_op);
            tauri::async_runtime::spawn(async move { me.pick().await });
        }
        Ok(UpdateOutcome::Updated(latest))
    }

    /// After sleep or hibernation: tg-ws-proxy keeps pooled WebSocket and H2
    /// connections that died while the PC slept, and its retry back-off has grown
    /// on the failures, so Telegram stays slow or offline until they time out.
    /// A fresh start costs a second. winws.exe is restarted too, so WinDivert
    /// starts clean on the network that came back.
    async fn after_sleep(&self, gap: Duration) {
        // network adapters need a moment after waking up
        tokio::time::sleep(Duration::from_secs(5)).await;
        let (tg, bypass) = {
            let g = self.lock();
            (
                g.settings.telegram && g.tg.is_some(),
                g.settings.enabled && g.settings.strategy.is_some() && g.progress.is_none(),
            )
        };
        if !tg && !bypass {
            return;
        }
        let what = match (tg, bypass) {
            (true, true) => "обход и ускорение Telegram",
            (true, false) => "ускорение Telegram",
            _ => "обход",
        };
        self.log(format!("Компьютер вышел из сна (пауза {} мин) — перезапускаю {what}", gap.as_secs() / 60));
        if tg {
            self.tg_start();
        }
        if bypass {
            // a pick or an update in progress restarts winws.exe by itself
            if let Ok(_op) = self.op.try_lock() {
                self.start_current();
            }
        }
        self.emit_state();
    }

    /// Every 15 s: restarts winws.exe if it died. Every 10 min: if Discord and
    /// YouTube stopped answering while the internet works, picks again.
    pub async fn watchdog(self: Arc<Self>) {
        let mut tick: u64 = 0;
        let mut strikes = 0;
        // restarts in a row that did not keep winws.exe alive (antivirus, broken driver…)
        let mut revives = 0;
        let mut tg_revives = 0;
        let mut last_tick = std::time::SystemTime::now();
        loop {
            tokio::time::sleep(Duration::from_secs(15)).await;
            tick += 1;

            // the wall clock jumped far past the 15 s: the PC was asleep
            let now = std::time::SystemTime::now();
            let gap = now.duration_since(last_tick).unwrap_or_default();
            last_tick = now;
            if gap > Duration::from_secs(60) {
                strikes = 0;
                revives = 0;
                tg_revives = 0;
                self.after_sleep(gap).await;
                last_tick = std::time::SystemTime::now();
                continue;
            }

            // tg-ws-proxy: bring it back if it died, three tries at most
            let (tg_want, tg_alive) = {
                let mut g = self.lock();
                (g.settings.telegram, g.tg_runner.is_running())
            };
            if !tg_want || tg_alive {
                tg_revives = 0;
            } else if tg_revives < 3 {
                tg_revives += 1;
                if tg_revives == 3 {
                    self.log(format!("tg-ws-proxy не держится — перестаю перезапускать ({})", tgws::console_tail()));
                    self.lock().tg_error = Some("tg-ws-proxy закрывается сам. Подробности в журнале".into());
                } else {
                    self.log("tg-ws-proxy остановился — перезапускаю");
                    self.tg_start();
                }
                self.emit_state();
            }

            let (want, alive, picking) = {
                let mut g = self.lock();
                (g.settings.enabled && g.settings.strategy.is_some(), g.runner.is_running(), g.progress.is_some())
            };
            if !want || picking {
                strikes = 0;
                revives = 0;
                continue;
            }
            if !alive {
                if revives >= 3 {
                    continue;
                }
                if let Ok(_g) = self.op.try_lock() {
                    revives += 1;
                    if revives == 3 {
                        self.log("winws.exe не держится — перестаю перезапускать. Проверьте антивирус и включите обход снова");
                        self.lock().error = Some("winws.exe закрывается сам. Проверьте, не удалил ли антивирус WinDivert".into());
                        self.emit_state();
                        continue;
                    }
                    self.log("winws.exe остановился — перезапускаю");
                    self.start_current();
                    self.emit_state();
                }
                continue;
            }
            revives = 0;
            if tick % 40 != 0 {
                continue;
            }
            let Ok(guard) = self.op.try_lock() else { continue };
            let s = checker::check_all(4000).await;
            drop(guard);
            if s.ok == 0 && checker::internet_ok().await {
                strikes += 1;
                if strikes >= 2 {
                    strikes = 0;
                    self.log("Discord и YouTube перестали открываться — подбираю стратегию заново");
                    self.clone().pick().await;
                }
            } else {
                strikes = 0;
            }
        }
    }

    pub async fn auto_update_loop(self: Arc<Self>) {
        tokio::time::sleep(Duration::from_secs(30)).await;
        loop {
            if self.settings().auto_update {
                // offline or GitHub unreachable: try again later, quietly
                let _ = self.clone().update().await;
                let _ = self.clone().tg_update().await;
                // kidpi itself: installs right away while it sits in the tray,
                // otherwise Settings offers the update
                if let Ok(Some(v)) = self.app_check().await {
                    let visible = self
                        .app
                        .get_webview_window("main")
                        .and_then(|w| w.is_visible().ok())
                        .unwrap_or(false);
                    if visible {
                        self.log(format!("Доступна новая версия kidpi {v}"));
                    } else if let Err(e) = self.app_install().await {
                        self.log(format!("Обновление kidpi: {e}"));
                    }
                }
            }
            tokio::time::sleep(Duration::from_secs(6 * 3600)).await;
        }
    }
}
