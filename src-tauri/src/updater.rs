//! Updates zapret from Flowseal/zapret-discord-youtube, the same way its own
//! service.bat checks: `.service/version.txt` on the main branch, then the
//! release archive for that tag.

use std::fs;
use std::io::{Cursor, Read};
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use crate::{paths, zapret};

const VERSION_URL: &str = "https://raw.githubusercontent.com/Flowseal/zapret-discord-youtube/main/.service/version.txt";

fn release_url(ver: &str) -> String {
    format!("https://github.com/Flowseal/zapret-discord-youtube/releases/download/{ver}/zapret-discord-youtube-{ver}.zip")
}

fn client(timeout: Duration) -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(timeout)
        .user_agent("kidpi")
        .build()
        .map_err(|e| e.to_string())
}

pub async fn latest_version() -> Result<String, String> {
    let c = client(Duration::from_secs(15))?;
    let text = c
        .get(VERSION_URL)
        .header("Cache-Control", "no-cache")
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("нет связи с GitHub: {e}"))?
        .text()
        .await
        .map_err(|e| e.to_string())?;
    let v = text.trim().to_string();
    if v.is_empty() || v.len() > 32 || !v.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-') {
        return Err("GitHub вернул непонятный номер версии".into());
    }
    Ok(v)
}

/// Downloads release `ver` and unpacks it into `zapret\<ver>`, keeping the
/// user's own lists from `current_dir`. Returns the new folder.
pub async fn install(ver: &str, current_dir: &Path) -> Result<PathBuf, String> {
    let c = client(Duration::from_secs(180))?;
    let bytes = c
        .get(release_url(ver))
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("не удалось скачать zapret {ver}: {e}"))?
        .bytes()
        .await
        .map_err(|e| format!("загрузка прервалась: {e}"))?;

    let root = paths::zapret_root();
    let dir = zapret::fresh_dir(&root, ver);
    let tmp = root.join(format!("{}.tmp", dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()));
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;

    unpack(&bytes, &tmp).map_err(|e| {
        let _ = fs::remove_dir_all(&tmp);
        format!("архив zapret повреждён: {e}")
    })?;
    if !zapret::is_valid(&tmp) {
        let _ = fs::remove_dir_all(&tmp);
        return Err("в архиве нет winws.exe — возможно, его удалил антивирус".into());
    }
    let _ = fs::write(tmp.join("version.txt"), ver);
    zapret::carry_user_lists(current_dir, &tmp);
    zapret::ensure_user_lists(&tmp);

    fs::rename(&tmp, &dir).map_err(|e| format!("не удалось установить zapret: {e}"))?;
    Ok(dir)
}

/// The archive holds one top folder (`zapret-discord-youtube-<ver>/…`); it is stripped.
fn unpack(bytes: &[u8], to: &Path) -> Result<(), String> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| e.to_string())?;
    for i in 0..zip.len() {
        let mut f = zip.by_index(i).map_err(|e| e.to_string())?;
        let Some(name) = f.enclosed_name() else { continue };
        let rel: PathBuf = name.components().skip(1).filter(|c| matches!(c, Component::Normal(_))).collect();
        if rel.as_os_str().is_empty() {
            continue;
        }
        let out = to.join(&rel);
        if f.is_dir() {
            fs::create_dir_all(&out).map_err(|e| e.to_string())?;
            continue;
        }
        if let Some(p) = out.parent() {
            fs::create_dir_all(p).map_err(|e| e.to_string())?;
        }
        let mut buf = Vec::with_capacity(f.size() as usize);
        f.read_to_end(&mut buf).map_err(|e| e.to_string())?;
        fs::write(&out, buf).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Keeps the current and the previous version, removes older ones.
pub fn cleanup(keep: &[&Path]) {
    let Ok(rd) = fs::read_dir(paths::zapret_root()) else { return };
    for e in rd.filter_map(|e| e.ok()) {
        let p = e.path();
        if p.is_dir() && !keep.iter().any(|k| *k == p.as_path()) {
            let _ = fs::remove_dir_all(&p);
        }
    }
}
