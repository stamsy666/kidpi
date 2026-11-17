use serde::Serialize;
use std::collections::VecDeque;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter};

use crate::paths;

#[derive(Serialize, Clone, Debug)]
pub struct LogEntry {
    pub time: String,
    pub text: String,
}

/// The journal shown in Settings, mirrored to `%ProgramData%\kidpi\logs\kidpi.log`.
pub struct LogBuf {
    entries: Mutex<VecDeque<LogEntry>>,
}

const KEEP: usize = 200;
const MAX_FILE: u64 = 1024 * 1024;

impl LogBuf {
    pub fn new() -> Self {
        Self { entries: Mutex::new(VecDeque::new()) }
    }

    pub fn push(&self, app: &AppHandle, text: impl Into<String>) {
        let text = text.into();
        let now = chrono::Local::now();
        let entry = LogEntry { time: now.format("%H:%M").to_string(), text: text.clone() };

        let dir = paths::logs_dir();
        let _ = fs::create_dir_all(&dir);
        let file = dir.join("kidpi.log");
        if fs::metadata(&file).map(|m| m.len() > MAX_FILE).unwrap_or(false) {
            let _ = fs::rename(&file, dir.join("kidpi.old.log"));
        }
        if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&file) {
            let _ = writeln!(f, "{} {}", now.format("%Y-%m-%d %H:%M:%S"), text);
        }

        if let Ok(mut e) = self.entries.lock() {
            e.push_front(entry.clone());
            e.truncate(KEEP);
        }
        let _ = app.emit("log", &entry);
    }

    /// Newest first.
    pub fn list(&self) -> Vec<LogEntry> {
        self.entries.lock().map(|e| e.iter().cloned().collect()).unwrap_or_default()
    }
}
