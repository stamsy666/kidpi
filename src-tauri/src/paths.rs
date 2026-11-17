//! Where kidpi keeps its data.
//!
//! Everything lives in `%ProgramData%\kidpi`. kidpi runs elevated, so the
//! folders it creates there are owned by Administrators and a non-admin
//! process cannot swap `winws.exe` for something else.

use std::path::PathBuf;

pub fn base_dir() -> PathBuf {
    std::env::var_os("ProgramData")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"))
        .join("kidpi")
}

/// One sub-folder per zapret version: `zapret\1.10.4`, `zapret\1.10.5`, …
pub fn zapret_root() -> PathBuf {
    base_dir().join("zapret")
}

pub fn settings_file() -> PathBuf {
    base_dir().join("settings.json")
}

pub fn logs_dir() -> PathBuf {
    base_dir().join("logs")
}
