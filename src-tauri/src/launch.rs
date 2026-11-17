//! Opening links and programs as the signed-in user, not as admin.
//!
//! kidpi runs elevated (WinDivert needs it), so whatever it starts directly is
//! elevated too. Links therefore go through Explorer's own ShellExecute: the
//! desktop's automation object runs it inside explorer.exe, with the user's
//! rights and the user's file and link associations.
//!
//! (Passing a link to `explorer.exe <link>` is not the same thing: on current
//! Windows 11 Explorer treats a `tg://` argument as a folder it cannot find and
//! opens Documents.)

use std::path::{Path, PathBuf};

/// How `open_telegram` reached Telegram.
pub enum TgOpen {
    /// The `tg://` link went to the app registered for it.
    Link,
    /// No `tg://` handler, but Telegram Desktop was found and got the link directly.
    Exe,
    /// No Telegram found: the link was still handed to Windows, which offers to pick an app.
    Unknown,
}

/// Opens a `https://` link in the user's browser.
pub fn open_url(url: &str) -> Result<(), String> {
    shell_open(url, "")
}

/// Hands a `tg://proxy` link to Telegram Desktop so it offers to add the proxy.
pub fn open_telegram(link: &str) -> Result<TgOpen, String> {
    if has_tg_handler() {
        return shell_open(link, "").map(|_| TgOpen::Link);
    }
    if let Some(exe) = find_telegram() {
        let args = format!("-- \"{link}\"");
        return shell_open(&exe.to_string_lossy(), &args).map(|_| TgOpen::Exe);
    }
    shell_open(link, "").map(|_| TgOpen::Unknown)
}

/// ShellExecute in Explorer's process; if Explorer cannot be reached (another
/// shell, Explorer restarting), a plain ShellExecute from kidpi.
fn shell_open(file: &str, args: &str) -> Result<(), String> {
    #[cfg(windows)]
    {
        let (f, a) = (file.to_string(), args.to_string());
        let via_shell = std::thread::spawn(move || win::shell_execute_as_user(&f, &a))
            .join()
            .unwrap_or_else(|_| Err("поток ShellExecute упал".into()));
        match via_shell {
            Ok(()) => Ok(()),
            Err(e) => win::shell_execute(file, args).map_err(|e2| format!("{e}; {e2}")),
        }
    }
    #[cfg(not(windows))]
    {
        let _ = (file, args);
        Err("только для Windows".into())
    }
}

/// A `tg://` handler is registered: classic (Telegram Desktop, forks) or a Store app.
fn has_tg_handler() -> bool {
    #[cfg(windows)]
    {
        win::protocol_registered("tg")
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// Telegram Desktop that is running now, or installed in its usual places.
fn find_telegram() -> Option<PathBuf> {
    const RUNNING: &[&str] = &["telegram.exe", "ayugram.exe", "64gram.exe", "kotatogram.exe"];
    #[cfg(windows)]
    for name in RUNNING {
        if let Some(p) = win::process_path(name) {
            return Some(p);
        }
    }
    #[cfg(not(windows))]
    let _ = RUNNING;
    let env = |k: &str| std::env::var_os(k).map(PathBuf::from);
    let candidates = [
        env("APPDATA").map(|d| d.join("Telegram Desktop").join("Telegram.exe")),
        env("LOCALAPPDATA").map(|d| d.join("Telegram Desktop").join("Telegram.exe")),
        env("LOCALAPPDATA").map(|d| d.join("Programs").join("Telegram Desktop").join("Telegram.exe")),
        env("ProgramFiles").map(|d| d.join("Telegram Desktop").join("Telegram.exe")),
        env("ProgramFiles(x86)").map(|d| d.join("Telegram Desktop").join("Telegram.exe")),
    ];
    candidates.into_iter().flatten().find(|p| Path::new(p).is_file())
}

#[cfg(windows)]
mod win {
    use std::path::PathBuf;
    use windows::core::{Interface, BSTR, PCWSTR, PWSTR};
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, IDispatch, IServiceProvider, CLSCTX_LOCAL_SERVER,
        COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE,
    };
    use windows::Win32::System::Variant::VARIANT;
    use windows::Win32::UI::Shell::{
        AssocQueryStringW, IShellBrowser, IShellDispatch2, IShellFolderViewDual, IShellView, IShellWindows,
        ShellExecuteW, ShellWindows, ASSOCF_IS_PROTOCOL, ASSOCSTR_APPID, ASSOCSTR_COMMAND, CSIDL_DESKTOP,
        SID_STopLevelBrowser, SVGIO_BACKGROUND, SWC_DESKTOP, SWFO_NEEDDISPATCH,
    };
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// Raymond Chen's "launch unelevated from an elevated process": find the
    /// desktop's folder view, take its Shell.Application object and call
    /// ShellExecute on it. The call runs in explorer.exe.
    /// Must run on its own thread (it sets up COM there).
    pub fn shell_execute_as_user(file: &str, args: &str) -> Result<(), String> {
        unsafe {
            let init = CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE);
            let r = (|| -> windows::core::Result<()> {
                let windows: IShellWindows = CoCreateInstance(&ShellWindows, None, CLSCTX_LOCAL_SERVER)?;
                let loc = VARIANT::from(CSIDL_DESKTOP as i32);
                let root = VARIANT::default();
                let mut hwnd = 0i32;
                let disp = windows.FindWindowSW(&loc, &root, SWC_DESKTOP, &mut hwnd, SWFO_NEEDDISPATCH)?;
                let provider: IServiceProvider = disp.cast()?;
                let browser: IShellBrowser = provider.QueryService(&SID_STopLevelBrowser)?;
                let view: IShellView = browser.QueryActiveShellView()?;
                let view_disp: IDispatch = view.GetItemObject(SVGIO_BACKGROUND)?;
                let folder_view: IShellFolderViewDual = view_disp.cast()?;
                let shell: IShellDispatch2 = folder_view.Application()?.cast()?;
                shell.ShellExecute(
                    &BSTR::from(file),
                    &VARIANT::from(BSTR::from(args)),
                    &VARIANT::from(BSTR::new()),
                    &VARIANT::from(BSTR::new()),
                    &VARIANT::from(SW_SHOWNORMAL.0),
                )
            })();
            if init.is_ok() {
                CoUninitialize();
            }
            r.map_err(|e| format!("{:#010x}", e.code().0))
        }
    }

    /// Plain ShellExecute from kidpi itself (the program then runs as admin).
    pub fn shell_execute(file: &str, args: &str) -> Result<(), String> {
        let (f, a) = (wide(file), wide(args));
        let h = unsafe {
            ShellExecuteW(None, PCWSTR::null(), PCWSTR(f.as_ptr()), PCWSTR(a.as_ptr()), PCWSTR::null(), SW_SHOWNORMAL)
        };
        let code = h.0 as isize;
        if code > 32 {
            Ok(())
        } else {
            Err(format!("ShellExecute {code}"))
        }
    }

    pub fn protocol_registered(scheme: &str) -> bool {
        let s = wide(scheme);
        [ASSOCSTR_COMMAND, ASSOCSTR_APPID].into_iter().any(|what| {
            let mut buf = [0u16; 1024];
            let mut len = buf.len() as u32;
            let hr = unsafe {
                AssocQueryStringW(ASSOCF_IS_PROTOCOL, what, PCWSTR(s.as_ptr()), PCWSTR::null(), Some(PWSTR(buf.as_mut_ptr())), &mut len)
            };
            hr.is_ok() && len > 1 && buf[0] != 0
        })
    }

    /// Full path of a running process with this exe name (lowercase).
    pub fn process_path(name: &str) -> Option<PathBuf> {
        use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
        use windows_sys::Win32::System::Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
        };
        use windows_sys::Win32::System::Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION};

        let mut found = None;
        unsafe {
            let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
            if snap == INVALID_HANDLE_VALUE {
                return None;
            }
            let mut e: PROCESSENTRY32W = std::mem::zeroed();
            e.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
            let mut more = Process32FirstW(snap, &mut e) != 0;
            while more && found.is_none() {
                let len = e.szExeFile.iter().position(|&c| c == 0).unwrap_or(e.szExeFile.len());
                if String::from_utf16_lossy(&e.szExeFile[..len]).to_lowercase() == name {
                    let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, e.th32ProcessID);
                    if !h.is_null() {
                        let mut buf = [0u16; 1024];
                        let mut n = buf.len() as u32;
                        if QueryFullProcessImageNameW(h, 0, buf.as_mut_ptr(), &mut n) != 0 {
                            found = Some(PathBuf::from(String::from_utf16_lossy(&buf[..n as usize])));
                        }
                        CloseHandle(h);
                    }
                }
                more = Process32NextW(snap, &mut e) != 0;
            }
            CloseHandle(snap);
        }
        found
    }
}
