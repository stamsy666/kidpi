use serde::{Deserialize, Serialize};
use std::fs;

use crate::paths;

/// Where kidpi's own releases are published (see ИНСТРУКЦИЯ.md to change it).
pub const DEFAULT_UPDATE_REPO: &str = "stamsy666/kidpi";
/// Placeholder repo 0.3.0 saved into settings before the real one was known.
const OLD_UPDATE_REPOS: &[&str] = &["kidencedev/kidpi"];

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    /// Display name of the chosen strategy, e.g. "ALT 7". None until the first pick.
    pub strategy: Option<String>,
    /// Whether the bypass should be running (restored on the next start).
    pub enabled: bool,
    pub auto_update: bool,
    pub autostart: bool,
    pub close_to_tray: bool,
    pub game_mode: bool,
    pub sounds: bool,
    /// The "kidpi работает в трее" message was already shown once.
    pub tray_notified: bool,
    /// Installed zapret version in use, e.g. "1.10.4".
    pub zapret_version: Option<String>,
    /// "Ускорение Telegram" (tg-ws-proxy) is on.
    pub telegram: bool,
    pub tg_port: u16,
    /// MTProto secret of the local proxy; made once, so Telegram keeps working after restarts.
    pub tg_secret: Option<String>,
    /// tg-ws-proxy version in use, e.g. "1.11.4".
    pub tg_version: Option<String>,
    /// GitHub repository ("owner/name") whose releases update kidpi itself.
    pub update_repo: String,
    /// Interface scale, 1.0 = 100 % (see scale.rs).
    pub scale: f64,
    /// The intro plays whenever the main window opens (at launch, from the tray or taskbar).
    pub intro: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            strategy: None,
            enabled: true,
            auto_update: true,
            autostart: true,
            close_to_tray: true,
            game_mode: false,
            sounds: true,
            tray_notified: false,
            zapret_version: None,
            telegram: false,
            tg_port: 1443,
            tg_secret: None,
            tg_version: None,
            update_repo: DEFAULT_UPDATE_REPO.into(),
            scale: 1.0,
            intro: true,
        }
    }
}

impl Settings {
    pub fn load() -> Self {
        let mut s: Settings = fs::read_to_string(paths::settings_file())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        let repo = s.update_repo.trim();
        if repo.is_empty() || OLD_UPDATE_REPOS.contains(&repo) {
            s.update_repo = DEFAULT_UPDATE_REPO.into();
        }
        s.scale = crate::scale::snap(s.scale);
        s
    }

    pub fn save(&self) {
        let path = paths::settings_file();
        if let Some(dir) = path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let tmp = path.with_extension("json.tmp");
            if fs::write(&tmp, json).is_ok() {
                let _ = fs::rename(&tmp, &path);
            }
        }
    }
}
