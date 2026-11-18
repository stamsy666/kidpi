//! Everything about the zapret bundle from Flowseal/zapret-discord-youtube:
//! where it is installed, which strategies it has, and running `winws.exe`.
//!
//! kidpi does not run the `.bat` files. It reads each `general*.bat`, takes the
//! `winws.exe` command line out of it and starts `winws.exe` directly, hidden.
//! New strategies that arrive with an update therefore show up on their own.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use crate::paths;

#[derive(Clone, Debug)]
pub struct Strategy {
    /// What the user sees: "general", "ALT 7", "FAKE TLS AUTO ALT 2", …
    pub name: String,
    pub file: PathBuf,
}

#[derive(Clone, Debug)]
pub struct Install {
    pub dir: PathBuf,
    pub version: String,
}

/// Port ranges zapret's service.bat uses for its "Game Filter".
const GAME_RANGE: &str = "1024-65535";
/// service.bat puts this harmless port in when the game filter is off.
const GAME_OFF: &str = "12";

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub fn read_version(dir: &Path) -> Option<String> {
    fs::read_to_string(dir.join("version.txt")).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

pub fn is_valid(dir: &Path) -> bool {
    dir.join("bin").join("winws.exe").is_file() && dir.join("lists").is_dir()
}

/// "1.10.4" > "1.10.3"; non-numeric parts compare as 0.
pub fn newer(a: &str, b: &str) -> bool {
    let p = |s: &str| s.trim().trim_start_matches('v').split('.').map(|x| x.parse::<u64>().unwrap_or(0)).collect::<Vec<_>>();
    let (a, b) = (p(a), p(b));
    for i in 0..a.len().max(b.len()) {
        let (x, y) = (a.get(i).copied().unwrap_or(0), b.get(i).copied().unwrap_or(0));
        if x != y {
            return x > y;
        }
    }
    false
}

pub fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

/// What the user sees when the installer's own copy of zapret is unusable.
pub const AV_HINT: &str = "Антивирус удалил winws.exe или WinDivert из папки kidpi. Добавьте папки C:\\Program Files\\kidpi и C:\\ProgramData\\kidpi в исключения антивируса и переустановите kidpi";

/// A folder for version `ver` under the zapret root: `ver`, or `ver-2`, `ver-3`… when a
/// broken leftover cannot be removed (a file still held by the WinDivert driver).
pub fn fresh_dir(root: &Path, ver: &str) -> PathBuf {
    for n in 1..50 {
        let name = if n == 1 { ver.to_string() } else { format!("{ver}-{n}") };
        let d = root.join(&name);
        if !d.exists() {
            return d;
        }
        let _ = fs::remove_dir_all(&d);
        if !d.exists() {
            return d;
        }
    }
    root.join(format!("{ver}-{}", std::process::id()))
}

/// The key kept in settings for an installed folder: its name under the zapret
/// root, or None when zapret runs from the installer's folder.
pub fn folder_key(inst: &Install) -> Option<String> {
    if inst.dir.starts_with(paths::zapret_root()) {
        inst.dir.file_name().map(|n| n.to_string_lossy().to_string())
    } else {
        None
    }
}

fn copy_out(bundled: &Path, ver: &str) -> Result<PathBuf, String> {
    let root = paths::zapret_root();
    fs::create_dir_all(&root).map_err(|e| format!("нет доступа к {}: {e}", root.display()))?;
    let dir = fresh_dir(&root, ver);
    let tmp = root.join(format!("{}.tmp", dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()));
    let _ = fs::remove_dir_all(&tmp);
    copy_dir(bundled, &tmp).map_err(|e| {
        let _ = fs::remove_dir_all(&tmp);
        format!("копирование: {e}")
    })?;
    fs::rename(&tmp, &dir).map_err(|e| format!("перенос: {e}"))?;
    if !is_valid(&dir) {
        return Err("после копирования пропал winws.exe".into());
    }
    Ok(dir)
}

/// Makes sure a usable zapret exists, normally in `%ProgramData%\kidpi\zapret\<version>`
/// (a path without spaces, as zapret asks). The installer ships zapret as a resource; it
/// is copied out on first run, or when kidpi itself brought a newer zapret. If the copy
/// is impossible, zapret runs from the installer's folder instead of not at all.
/// Returns the install and, when something went wrong but kidpi coped, a warning.
pub fn ensure_installed(bundled: &Path, preferred: Option<&str>) -> Result<(Install, Option<String>), String> {
    let root = paths::zapret_root();
    let bundled_ver = read_version(bundled);
    let bundled_ok = is_valid(bundled);
    let current = preferred
        .map(|v| root.join(v))
        .filter(|d| is_valid(d))
        .map(|d| Install { version: read_version(&d).unwrap_or_else(|| preferred.unwrap_or_default().to_string()), dir: d });

    let take_bundled = match (&current, &bundled_ver) {
        (None, Some(_)) => true,
        (Some(cur), Some(b)) => newer(b, &cur.version),
        _ => false,
    };

    if !take_bundled {
        return match current {
            Some(cur) => {
                ensure_user_lists(&cur.dir);
                Ok((cur, None))
            }
            None => Err("Файлы zapret не найдены в папке kidpi — переустановите kidpi".into()),
        };
    }

    let ver = bundled_ver.unwrap_or_else(|| "bundled".into());
    if !bundled_ok {
        return match current {
            Some(cur) => {
                ensure_user_lists(&cur.dir);
                Ok((cur, Some(AV_HINT.into())))
            }
            None => Err(AV_HINT.into()),
        };
    }

    match copy_out(bundled, &ver) {
        Ok(dir) => {
            if let Some(cur) = &current {
                carry_user_lists(&cur.dir, &dir);
            }
            ensure_user_lists(&dir);
            Ok((Install { dir, version: ver }, None))
        }
        Err(e) => {
            if let Some(cur) = &current {
                carry_user_lists(&cur.dir, bundled);
            }
            ensure_user_lists(bundled);
            Ok((
                Install { dir: bundled.to_path_buf(), version: ver },
                Some(format!("Не удалось скопировать zapret в C:\\ProgramData\\kidpi ({e}) — запускаю его из папки установки")),
            ))
        }
    }
}

/// The same placeholder files zapret's `service.bat load_user_lists` creates:
/// winws refuses to start with an empty or missing list.
pub fn ensure_user_lists(dir: &Path) {
    let lists = dir.join("lists");
    let _ = fs::create_dir_all(&lists);
    let files = [
        ("ipset-exclude-user.txt", "203.0.113.113/32\r\n"),
        ("list-general-user.txt", "# Never leave this file empty\r\ndomain.example.abc\r\n"),
        ("list-exclude-user.txt", "domain.example.abc\r\n"),
    ];
    for (name, content) in files {
        let p = lists.join(name);
        if !p.exists() {
            let _ = fs::write(p, content);
        }
    }
}

/// Keeps the user's own lists when switching to a new zapret version.
pub fn carry_user_lists(from: &Path, to: &Path) {
    let _ = fs::create_dir_all(to.join("lists"));
    for name in ["list-general-user.txt", "list-exclude-user.txt", "ipset-exclude-user.txt"] {
        let src = from.join("lists").join(name);
        if src.is_file() {
            let _ = fs::copy(&src, to.join("lists").join(name));
        }
    }
}

/// "general (FAKE TLS AUTO ALT2).bat" -> "FAKE TLS AUTO ALT 2"; "general.bat" -> "general".
fn display_name(stem: &str) -> String {
    let inner = match (stem.find('('), stem.rfind(')')) {
        (Some(a), Some(b)) if b > a => stem[a + 1..b].trim().to_string(),
        _ => stem.trim().to_string(),
    };
    let mut out = String::new();
    let chars: Vec<char> = inner.chars().collect();
    for (i, c) in chars.iter().enumerate() {
        if c.is_ascii_digit() && i > 0 && chars[i - 1].is_alphabetic() {
            out.push(' ');
        }
        out.push(*c);
    }
    out
}

/// Natural order like service.bat: "ALT 2" before "ALT 10"; "general" first.
fn sort_key(name: &str) -> (u8, String) {
    let mut key = String::new();
    let mut num = String::new();
    for c in name.chars().chain(std::iter::once('\0')) {
        if c.is_ascii_digit() {
            num.push(c);
            continue;
        }
        if !num.is_empty() {
            key.push_str(&format!("{:0>8}", num));
            num.clear();
        }
        if c != '\0' {
            key.push(c.to_ascii_lowercase());
        }
    }
    (if name == "general" { 0 } else { 1 }, key)
}

pub fn list_strategies(dir: &Path) -> Vec<Strategy> {
    let mut out: Vec<Strategy> = fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| {
                    let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("").to_ascii_lowercase();
                    name.starts_with("general") && name.ends_with(".bat")
                })
                .filter(|p| read_args(p).is_some())
                .map(|p| {
                    let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
                    Strategy { name: display_name(&stem), file: p }
                })
                .collect()
        })
        .unwrap_or_default();
    out.sort_by_key(|s| sort_key(&s.name));
    out
}

/// Pulls the arguments of the `winws.exe` call out of a strategy `.bat`,
/// joining the `^`-continued lines.
pub fn read_args(bat: &Path) -> Option<String> {
    let text = fs::read_to_string(bat).ok()?;
    let text = text.trim_start_matches('\u{feff}');
    let marker = "winws.exe\"";
    let mut cur: Option<String> = None;
    for line in text.lines() {
        let l = line.trim_end();
        match cur.as_mut() {
            None => {
                if let Some(i) = l.find(marker) {
                    cur = Some(l[i + marker.len()..].to_string());
                } else {
                    continue;
                }
            }
            Some(c) => {
                c.push(' ');
                c.push_str(l.trim_start());
            }
        }
        let c = cur.as_mut().unwrap();
        if c.ends_with('^') {
            c.pop();
            continue;
        }
        break;
    }
    cur.map(|c| c.trim().to_string()).filter(|c| !c.is_empty())
}

/// What cmd.exe would do to the line: `^x` -> `x` outside quotes, `%%` -> `%`.
fn cmd_unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut quoted = false;
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '"' => {
                quoted = !quoted;
                out.push(c);
            }
            '^' if !quoted => {
                if let Some(n) = it.next() {
                    out.push(n);
                }
            }
            '%' if it.peek() == Some(&'%') => {
                it.next();
                out.push('%');
            }
            _ => out.push(c),
        }
    }
    out
}

pub fn build_args(install_dir: &Path, strategy: &Strategy, game_mode: bool) -> Option<String> {
    let raw = read_args(&strategy.file)?;
    let bin = format!("{}\\", install_dir.join("bin").display());
    let lists = format!("{}\\", install_dir.join("lists").display());
    let game = if game_mode { GAME_RANGE } else { GAME_OFF };
    let s = raw
        .replace("%BIN%", &bin)
        .replace("%LISTS%", &lists)
        .replace("%GameFilterTCP%", game)
        .replace("%GameFilterUDP%", game)
        .replace("%GameFilter%", game);
    Some(cmd_unescape(&s))
}

/// The running `winws.exe`. It is put into a Windows job object, so it dies
/// together with kidpi even if kidpi crashes.
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
            job: job::create(),
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

    pub fn start(&mut self, install: &Install, strategy: &Strategy, game_mode: bool) -> Result<(), String> {
        self.stop();
        let args = build_args(&install.dir, strategy, game_mode).ok_or("не удалось прочитать стратегию")?;
        let bin = install.dir.join("bin");
        let exe = bin.join("winws.exe");
        if !exe.is_file() {
            return Err("winws.exe не найден — возможно, его удалил антивирус".into());
        }

        let logs = paths::logs_dir();
        let _ = fs::create_dir_all(&logs);
        let out = fs::File::create(logs.join("winws.log")).map(Stdio::from).unwrap_or_else(|_| Stdio::null());
        let err = fs::OpenOptions::new()
            .append(true)
            .open(logs.join("winws.log"))
            .map(Stdio::from)
            .unwrap_or_else(|_| Stdio::null());

        let mut cmd = Command::new(&exe);
        cmd.current_dir(&bin).stdin(Stdio::null()).stdout(out).stderr(err);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(CREATE_NO_WINDOW).raw_arg(&args);
        }
        #[cfg(not(windows))]
        {
            cmd.arg(&args);
        }
        let child = cmd.spawn().map_err(|e| format!("не удалось запустить winws.exe: {e}"))?;
        #[cfg(windows)]
        if let Some(job) = self.job {
            job::assign(job, &child);
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

/// Stops any winws.exe that kidpi did not start (another zapret, a crashed
/// kidpi, the zapret service from service.bat). Two of them at once break both.
pub fn kill_stray() {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let _ = Command::new("taskkill")
            .args(["/F", "/IM", "winws.exe"])
            .creation_flags(CREATE_NO_WINDOW)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

/// Turns on TCP timestamps (RFC 1323) in Windows, as zapret's own strategy files do:
/// each `general*.bat` calls `service.bat status_zapret`, which runs
/// `netsh interface tcp set global timestamps=enabled`. Most strategies use
/// `--dpi-desync-fooling=ts`: the fake packets carry an outdated timestamp, so the
/// server drops them and only the DPI sees them. Windows keeps timestamps off by
/// default; then the fakes reach the server and break the connection, so sites open
/// one time and not the next. kidpi starts winws.exe itself, without the .bat,
/// so it has to do this on its own. Runs once per kidpi start.
/// Returns a line for the journal when something changed or went wrong.
pub fn enable_tcp_timestamps() -> Option<String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        use std::sync::atomic::{AtomicBool, Ordering};
        static DONE: AtomicBool = AtomicBool::new(false);
        if DONE.swap(true, Ordering::SeqCst) {
            return None;
        }
        let netsh = |args: &[&str]| {
            Command::new("netsh")
                .args(args)
                .creation_flags(CREATE_NO_WINDOW)
                .stdin(Stdio::null())
                .stderr(Stdio::null())
                .output()
        };
        // "RFC 1323 Timestamps : disabled"; the words around the value may be translated
        let state = netsh(&["interface", "tcp", "show", "global"]).ok().and_then(|o| {
            let text = String::from_utf8_lossy(&o.stdout).to_lowercase();
            let line = text.lines().find(|l| l.contains("1323"))?.to_string();
            if line.contains("disabled") {
                Some(false)
            } else if line.contains("enabled") {
                Some(true)
            } else {
                None
            }
        });
        if state == Some(true) {
            return None;
        }
        match netsh(&["interface", "tcp", "set", "global", "timestamps=enabled"]) {
            Ok(o) if o.status.success() => {
                (state == Some(false)).then(|| "Включил TCP timestamps в Windows: без них большинство стратегий zapret работают с перебоями. Если обход работал плохо, подберите стратегию заново".to_string())
            }
            Ok(o) => Some(format!(
                "Не удалось включить TCP timestamps (netsh: {}) — стратегии с fooling=ts могут работать с перебоями",
                String::from_utf8_lossy(&o.stdout).trim()
            )),
            Err(e) => Some(format!("Не удалось включить TCP timestamps: {e}")),
        }
    }
    #[cfg(not(windows))]
    {
        None
    }
}

/// Ends every process whose program lies inside `dir`.
pub fn kill_under(dir: &Path) -> usize {
    let prefix = format!("{}\\", dir.to_string_lossy().trim_end_matches('\\').to_lowercase());
    kill_matching(&|path: &str| path.starts_with(&prefix), None)
}

/// Ends every process started from exactly `exe` (full path, case-insensitive).
pub fn kill_by_path(exe: &Path) -> usize {
    let want = exe.to_string_lossy().to_lowercase();
    let file = exe.file_name().map(|f| f.to_string_lossy().to_lowercase());
    kill_matching(&|path: &str| path == want, file.as_deref())
}

/// Ends processes whose full program path satisfies `matches`; `name` narrows the
/// scan to one file name first.
/// Used for a tg-ws-proxy left over from a kidpi that was killed: matching by
/// path never touches anyone else's python.exe.
#[cfg(windows)]
fn kill_matching(matches: &dyn Fn(&str) -> bool, name: Option<&str>) -> usize {
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
    };
    use windows_sys::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, TerminateProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_TERMINATE,
    };

    let mut killed = 0;
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap == INVALID_HANDLE_VALUE {
            return 0;
        }
        let mut e: PROCESSENTRY32W = std::mem::zeroed();
        e.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut more = Process32FirstW(snap, &mut e) != 0;
        while more {
            let len = e.szExeFile.iter().position(|&c| c == 0).unwrap_or(e.szExeFile.len());
            let exe_name = String::from_utf16_lossy(&e.szExeFile[..len]).to_lowercase();
            if name.map_or(true, |n| n == exe_name) && e.th32ProcessID != std::process::id() {
                let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_TERMINATE, 0, e.th32ProcessID);
                if !h.is_null() {
                    let mut buf = [0u16; 1024];
                    let mut n = buf.len() as u32;
                    if QueryFullProcessImageNameW(h, 0, buf.as_mut_ptr(), &mut n) != 0 {
                        let path = String::from_utf16_lossy(&buf[..n as usize]).to_lowercase();
                        if matches(&path) && TerminateProcess(h, 1) != 0 {
                            killed += 1;
                        }
                    }
                    CloseHandle(h);
                }
            }
            more = Process32NextW(snap, &mut e) != 0;
        }
        CloseHandle(snap);
    }
    killed
}

#[cfg(not(windows))]
fn kill_matching(_matches: &dyn Fn(&str) -> bool, _name: Option<&str>) -> usize {
    0
}

/// Last lines winws.exe printed, for the journal when it fails to start.
pub fn winws_tail() -> String {
    let text = fs::read_to_string(paths::logs_dir().join("winws.log")).unwrap_or_default();
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    lines[lines.len().saturating_sub(2)..].join(" / ")
}

#[cfg(windows)]
pub(crate) mod job {
    use std::os::windows::io::AsRawHandle;
    use std::process::Child;
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation, SetInformationJobObject,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };

    pub fn create() -> Option<isize> {
        unsafe {
            let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if job.is_null() {
                return None;
            }
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const core::ffi::c_void,
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            );
            Some(job as isize)
        }
    }

    pub fn assign(job: isize, child: &Child) {
        unsafe {
            AssignProcessToJobObject(job as _, child.as_raw_handle() as _);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert_eq!(display_name("general (FAKE TLS AUTO ALT2)"), "FAKE TLS AUTO ALT 2");
        assert_eq!(display_name("general (ALT10)"), "ALT 10");
        assert_eq!(display_name("general"), "general");
        assert!(sort_key("ALT 2") < sort_key("ALT 10"));
        assert!(sort_key("general") < sort_key("ALT"));
    }

    #[test]
    fn unescape() {
        assert_eq!(cmd_unescape("--a=^! --b=\"x^y\""), "--a=! --b=\"x^y\"");
    }

    #[test]
    fn versions() {
        assert!(newer("1.10.5", "1.10.4"));
        assert!(newer("1.11", "1.10.9"));
        assert!(!newer("1.10.4", "1.10.4"));
    }
}
