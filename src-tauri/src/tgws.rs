//! "Ускорение Telegram": Flowseal's tg-ws-proxy (https://github.com/Flowseal/tg-ws-proxy, MIT).
//!
//! Their Windows release is a tray app of its own, so kidpi runs only the
//! proxy part (`proxy/tg_ws_proxy.py`) on a bundled Python, hidden. Telegram
//! Desktop connects to it as a local MTProto proxy through a `tg://proxy` link.
//! Updates come from the same GitHub releases: the source of the new tag
//! replaces the proxy code; the Python runtime and libraries stay.

use std::fs;
use std::io::{Cursor, Read};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Component, Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use crate::{paths, zapret};

pub const HOST: &str = "127.0.0.1";
/// Telegram's own data centres the proxy talks to by default (same as tg-ws-proxy).
const DC_IPS: &[&str] = &["2:149.154.167.220", "4:149.154.167.220"];
const LATEST_URL: &str = "https://github.com/Flowseal/tg-ws-proxy/releases/latest";

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Clone, Debug)]
pub struct Install {
    pub runtime: PathBuf,
    pub app: PathBuf,
    pub version: String,
    /// kidpi's own additions to the proxy (`kidpi_tweaks.py`), kept apart from the
    /// proxy code so updates of tg-ws-proxy do not remove them.
    pub tweaks: PathBuf,
}

fn root() -> PathBuf {
    paths::base_dir().join("tgws")
}

fn app_valid(dir: &Path) -> bool {
    dir.join("proxy").join("tg_ws_proxy.py").is_file()
}

/// Python and its libraries run straight from the installer's folder; only proxy
/// code that came with a later update lives in `%ProgramData%\kidpi\tgws\app\<version>`.
pub fn ensure_installed(bundled: &Path, preferred: Option<&str>) -> Result<Install, String> {
    let runtime = bundled.join("runtime");
    let tweaks = bundled.join("kidpi");
    if !runtime.join("python.exe").is_file() {
        return Err("в папке kidpi нет Python для tg-ws-proxy — переустановите kidpi или проверьте антивирус".into());
    }
    let bundled_app = bundled.join("app");
    let bundled_ver = zapret::read_version(&bundled_app).filter(|_| app_valid(&bundled_app));
    let updated = preferred
        .map(|v| root().join("app").join(v))
        .filter(|d| app_valid(d))
        .map(|d| (zapret::read_version(&d).unwrap_or_default(), d));
    match (updated, bundled_ver) {
        (Some((v, d)), Some(b)) if zapret::newer(&v, &b) => Ok(Install { runtime, app: d, version: v, tweaks }),
        (Some((v, d)), None) => Ok(Install { runtime, app: d, version: v, tweaks }),
        (_, Some(b)) => Ok(Install { runtime, app: bundled_app, version: b, tweaks }),
        _ => Err("tg-ws-proxy не найден в папке kidpi — переустановите kidpi".into()),
    }
}

/// Settings keep the folder name of updated proxy code; None for the bundled one.
pub fn folder_key(inst: &Install) -> Option<String> {
    if inst.app.starts_with(root()) {
        inst.app.file_name().map(|n| n.to_string_lossy().to_string())
    } else {
        None
    }
}

/// 32 hex characters, as tg-ws-proxy expects.
pub fn new_secret() -> String {
    let mut b = [0u8; 16];
    if getrandom::getrandom(&mut b).is_err() {
        let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos();
        b.copy_from_slice(&t.to_le_bytes());
    }
    b.iter().map(|x| format!("{x:02x}")).collect()
}

pub fn link(port: u16, secret: &str) -> String {
    format!("tg://proxy?server={HOST}&port={port}&secret=dd{secret}")
}

pub fn port_free(port: u16) -> bool {
    TcpListener::bind((HOST, port)).is_ok()
}

pub fn listening(port: u16) -> bool {
    let addr: SocketAddr = format!("{HOST}:{port}").parse().unwrap();
    TcpStream::connect_timeout(&addr, Duration::from_millis(800)).is_ok()
}

pub struct Runner {
    child: Option<Child>,
    #[cfg(windows)]
    job: Option<isize>,
}

impl Runner {
    pub fn new() -> Self {
        Self {
            child: None,
            #[cfg(windows)]
            job: zapret::job::create(),
        }
    }

    pub fn is_running(&mut self) -> bool {
        match self.child.as_mut() {
            Some(c) => match c.try_wait() {
                Ok(None) => true,
                _ => {
                    self.child = None;
                    false
                }
            },
            None => false,
        }
    }

    pub fn start(&mut self, inst: &Install, port: u16, secret: &str) -> Result<(), String> {
        self.stop();
        let python = inst.runtime.join("python.exe");
        if !port_free(port) {
            // a proxy left over from a kidpi that was killed holds the port
            if zapret::kill_by_path(&python) > 0 {
                std::thread::sleep(Duration::from_millis(400));
            }
            if !port_free(port) {
                return Err(format!("порт {port} занят — закройте приложение TgWsProxy, если оно запущено"));
            }
        }
        if !python.is_file() {
            return Err("не найден Python для tg-ws-proxy — возможно, его удалил антивирус".into());
        }
        let logs = paths::logs_dir();
        let _ = fs::create_dir_all(&logs);
        let out = fs::File::create(logs.join("tgws-console.log")).map(Stdio::from).unwrap_or_else(|_| Stdio::null());
        let err = fs::OpenOptions::new()
            .append(true)
            .open(logs.join("tgws-console.log"))
            .map(Stdio::from)
            .unwrap_or_else(|_| Stdio::null());

        // kidpi_tweaks is optional: without the file the proxy runs as upstream wrote it
        let boot = format!(
            "import sys,runpy,importlib.util as u;sys.path.insert(0,r'{}');sys.path.append(r'{}');\
             u.find_spec('kidpi_tweaks') and __import__('kidpi_tweaks');\
             runpy.run_module('proxy.tg_ws_proxy',run_name='__main__',alter_sys=True)",
            inst.app.display(),
            inst.tweaks.display()
        );
        let mut cmd = Command::new(&python);
        cmd.current_dir(&inst.app)
            .arg("-B")
            .arg("-c")
            .arg(boot)
            .args(["--host", HOST, "--port", &port.to_string(), "--secret", secret])
            .arg("--log-file")
            .arg(logs.join("tgws.log"));
        for dc in DC_IPS {
            cmd.args(["--dc-ip", dc]);
        }
        cmd.stdin(Stdio::null()).stdout(out).stderr(err);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        let child = cmd.spawn().map_err(|e| format!("не удалось запустить tg-ws-proxy: {e}"))?;
        #[cfg(windows)]
        if let Some(job) = self.job {
            zapret::job::assign(job, &child);
        }
        self.child = Some(child);
        Ok(())
    }

    pub fn stop(&mut self) {
        if let Some(mut c) = self.child.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

impl Drop for Runner {
    fn drop(&mut self) {
        self.stop();
    }
}

/// On start: ends a tg-ws-proxy of ours that outlived a killed kidpi.
pub fn kill_leftovers(bundled: &Path) -> usize {
    zapret::kill_by_path(&bundled.join("runtime").join("python.exe"))
}

/// Last lines of the proxy's own console output, for the journal.
pub fn console_tail() -> String {
    let text = fs::read_to_string(paths::logs_dir().join("tgws-console.log")).unwrap_or_default();
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    lines[lines.len().saturating_sub(2)..].join(" / ")
}

/// Latest release tag, from the redirect of /releases/latest (no API rate limit).
pub async fn latest_tag() -> Result<String, String> {
    let c = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::none())
        .user_agent("kidpi")
        .build()
        .map_err(|e| e.to_string())?;
    let r = c.get(LATEST_URL).send().await.map_err(|e| format!("нет связи с GitHub: {e}"))?;
    let loc = r
        .headers()
        .get(reqwest::header::LOCATION)
        .and_then(|v| v.to_str().ok())
        .ok_or("GitHub не ответил ссылкой на релиз")?;
    let tag = loc.rsplit("/tag/").next().unwrap_or("").trim().to_string();
    if tag.is_empty() || tag.len() > 40 || !tag.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-') {
        return Err("GitHub вернул непонятный номер версии".into());
    }
    Ok(tag)
}

pub fn version_of(tag: &str) -> String {
    tag.trim_start_matches(['v', 'V']).to_string()
}

/// Downloads the source of `tag` and unpacks the proxy code into `app\<version>`.
pub async fn install_tag(tag: &str) -> Result<PathBuf, String> {
    let ver = version_of(tag);
    let url = format!("https://github.com/Flowseal/tg-ws-proxy/archive/refs/tags/{tag}.zip");
    let c = reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .user_agent("kidpi")
        .build()
        .map_err(|e| e.to_string())?;
    let bytes = c
        .get(&url)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("не удалось скачать tg-ws-proxy {ver}: {e}"))?
        .bytes()
        .await
        .map_err(|e| format!("загрузка прервалась: {e}"))?;

    let apps = root().join("app");
    fs::create_dir_all(&apps).map_err(|e| e.to_string())?;
    let dir = zapret::fresh_dir(&apps, &ver);
    let tmp = apps.join(format!("{}.tmp", dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()));
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(Cursor::new(&bytes[..])).map_err(|e| e.to_string())?;
    for i in 0..zip.len() {
        let mut f = zip.by_index(i).map_err(|e| e.to_string())?;
        let Some(name) = f.enclosed_name() else { continue };
        let rel: PathBuf = name.components().skip(1).filter(|c| matches!(c, Component::Normal(_))).collect();
        let top = rel.components().next().and_then(|c| c.as_os_str().to_str()).unwrap_or("");
        // only the code kidpi runs; the tray app, docs and tests are not needed
        if !(top == "proxy" || top == "utils" || top == "LICENSE") || f.is_dir() {
            continue;
        }
        let out = tmp.join(&rel);
        if let Some(p) = out.parent() {
            fs::create_dir_all(p).map_err(|e| e.to_string())?;
        }
        let mut buf = Vec::new();
        f.read_to_end(&mut buf).map_err(|e| e.to_string())?;
        fs::write(&out, buf).map_err(|e| e.to_string())?;
    }
    if !app_valid(&tmp) {
        let _ = fs::remove_dir_all(&tmp);
        return Err("в архиве tg-ws-proxy нет нужных файлов".into());
    }
    let _ = fs::write(tmp.join("version.txt"), &ver);
    fs::rename(&tmp, &dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

/// Keeps the current and the previous proxy code, removes older ones.
pub fn cleanup(keep: &[&Path]) {
    let Ok(rd) = fs::read_dir(root().join("app")) else { return };
    for e in rd.filter_map(|e| e.ok()) {
        let p = e.path();
        if p.is_dir() && !keep.iter().any(|k| *k == p.as_path()) {
            let _ = fs::remove_dir_all(&p);
        }
    }
}
