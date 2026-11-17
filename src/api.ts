import { invoke } from "@tauri-apps/api/core";

export type Phase = "pick" | "picking" | "on" | "off";

export interface Progress {
  index: number;
  total: number;
  name: string;
}

export interface AppState {
  phase: Phase;
  strategy: string | null;
  strategies: string[];
  progress: Progress | null;
  pickFailed: boolean;
  zapretVersion: string;
  appVersion: string;
  autoUpdate: boolean;
  autostart: boolean;
  closeToTray: boolean;
  gameMode: boolean;
  sounds: boolean;
  trayNotified: boolean;
  error: string | null;
  telegram: boolean;
  tgRunning: boolean;
  tgVersion: string;
  tgLink: string | null;
  tgError: string | null;
  appUpdate: string | null;
  /** Interface scale, 1 = 100 % */
  scale: number;
  /** Play the intro at launch */
  intro: boolean;
}

export interface LogEntry {
  time: string;
  text: string;
}

/** The scales the settings offer (same as STEPS in scale.rs). */
export const SCALE_STEPS = [1, 1.1, 1.25, 1.5, 1.75, 2];

export type SettingKey = "autoUpdate" | "autostart" | "closeToTray" | "gameMode" | "sounds" | "telegram" | "intro";
export type TrayAction = "toggle" | "open" | "repick" | "settings" | "quit";

export const api = {
  getState: () => invoke<AppState>("get_state"),
  startPick: () => invoke<void>("start_pick"),
  cancelPick: () => invoke<void>("cancel_pick"),
  setEnabled: (on: boolean) => invoke<AppState>("set_enabled", { on }),
  setStrategy: (name: string) => invoke<AppState>("set_strategy", { name }),
  setSetting: (key: SettingKey, value: boolean) => invoke<AppState>("set_setting", { key, value }),
  /** "latest" or "updated:<what was installed>" */
  checkUpdates: () => invoke<string>("check_updates"),
  /** Resolves with the domain; rejects with "invalid" | "exists" | "missing" | "io:…" */
  addSite: (input: string) => invoke<string>("add_site", { input }),
  removeSite: (input: string) => invoke<string>("remove_site", { input }),
  /** "link" | "exe" — Telegram got the link; "unknown" — Telegram Desktop was not found. Rejects with "open". */
  openTgLink: () => invoke<"link" | "exe" | "unknown">("open_tg_link"),
  openUrl: (url: string) => invoke<void>("open_url", { url }),
  installAppUpdate: () => invoke<void>("install_app_update"),
  getLog: () => invoke<LogEntry[]>("get_log"),
  openLogFolder: () => invoke<void>("open_log_folder"),
  markTrayNotified: () => invoke<void>("mark_tray_notified"),
  /** Resolves with the scale actually applied. */
  setScale: (scale: number) => invoke<number>("set_scale", { scale }),
  scaleMax: () => invoke<number>("scale_max"),
  syncZoom: () => invoke<void>("sync_zoom"),
  /** true once per launch when the intro should play (on and not started hidden) */
  takeIntro: () => invoke<boolean>("take_intro"),
  hideMain: () => invoke<void>("hide_main"),
  minimizeMain: () => invoke<void>("minimize_main"),
  showMain: () => invoke<void>("show_main"),
  quitApp: () => invoke<void>("quit_app"),
  trayAction: (action: TrayAction) => invoke<void>("tray_action", { action }),
};
