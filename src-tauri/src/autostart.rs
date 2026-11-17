//! "Запускать вместе с Windows".
//!
//! kidpi needs administrator rights, and an ordinary Run-key entry would ask
//! for UAC on every logon. A Task Scheduler task with "highest privileges"
//! starts it elevated silently. It also must not stop on battery, which the
//! plain `schtasks /Create` flags cannot express, hence the XML.

#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::process::{Command, Stdio};

const TASK: &str = "kidpi";
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn schtasks() -> Command {
    let mut c = Command::new("schtasks");
    c.stdout(Stdio::null()).stderr(Stdio::null()).stdin(Stdio::null());
    #[cfg(windows)]
    c.creation_flags(CREATE_NO_WINDOW);
    c
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

pub fn is_enabled() -> bool {
    schtasks().args(["/Query", "/TN", TASK]).status().map(|s| s.success()).unwrap_or(false)
}

pub fn enable() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let user = match (std::env::var("USERDOMAIN"), std::env::var("USERNAME")) {
        (Ok(d), Ok(u)) => format!("{d}\\{u}"),
        (_, Ok(u)) => u,
        _ => return Err("не удалось определить пользователя".into()),
    };
    let xml = format!(
        r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo><Description>kidpi</Description></RegistrationInfo>
  <Triggers>
    <LogonTrigger><Enabled>true</Enabled><UserId>{user}</UserId><Delay>PT5S</Delay></LogonTrigger>
  </Triggers>
  <Principals>
    <Principal id="Author"><UserId>{user}</UserId><LogonType>InteractiveToken</LogonType><RunLevel>HighestAvailable</RunLevel></Principal>
  </Principals>
  <Settings>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <AllowHardTerminate>true</AllowHardTerminate>
    <StartWhenAvailable>false</StartWhenAvailable>
    <RunOnlyIfNetworkAvailable>false</RunOnlyIfNetworkAvailable>
    <IdleSettings><StopOnIdleEnd>false</StopOnIdleEnd><RestartOnIdle>false</RestartOnIdle></IdleSettings>
    <AllowStartOnDemand>true</AllowStartOnDemand>
    <Enabled>true</Enabled>
    <Hidden>false</Hidden>
    <RunOnlyIfIdle>false</RunOnlyIfIdle>
    <WakeToRun>false</WakeToRun>
    <ExecutionTimeLimit>PT0S</ExecutionTimeLimit>
    <Priority>7</Priority>
  </Settings>
  <Actions Context="Author">
    <Exec><Command>{exe}</Command><Arguments>--autostart</Arguments></Exec>
  </Actions>
</Task>
"#,
        user = xml_escape(&user),
        exe = xml_escape(&exe.display().to_string()),
    );

    // Task Scheduler wants UTF-16 LE with a BOM.
    let mut bytes = vec![0xFF, 0xFE];
    for u in xml.encode_utf16() {
        bytes.extend_from_slice(&u.to_le_bytes());
    }
    let file = crate::paths::base_dir().join("autostart-task.xml");
    let _ = std::fs::create_dir_all(crate::paths::base_dir());
    std::fs::write(&file, bytes).map_err(|e| e.to_string())?;
    let file_arg = file.display().to_string();
    let ok = schtasks().args(["/Create", "/F", "/TN", TASK, "/XML", &file_arg]).status().map(|s| s.success()).unwrap_or(false);
    let _ = std::fs::remove_file(&file);
    if ok {
        Ok(())
    } else {
        Err("Планировщик заданий отказал".into())
    }
}

pub fn disable() -> Result<(), String> {
    if !is_enabled() {
        return Ok(());
    }
    let ok = schtasks().args(["/Delete", "/F", "/TN", TASK]).status().map(|s| s.success()).unwrap_or(false);
    if ok {
        Ok(())
    } else {
        Err("Планировщик заданий отказал".into())
    }
}
