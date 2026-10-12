//! Start with Windows, either as a normal program (HKCU Run key) or elevated through a
//! scheduled task. Elevated is what makes remapping work inside administrator windows
//! (Task Manager, admin terminals), without a UAC prompt at every logon.

use std::mem::{size_of, zeroed};
use std::os::windows::process::CommandExt;
use std::process::Command;
use std::ptr::null_mut;

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
use windows_sys::Win32::Security::{GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation};
use windows_sys::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE, REG_SZ, RRF_RT_REG_SZ, RegCloseKey, RegDeleteValueW,
    RegGetValueW, RegOpenKeyExW, RegSetValueExW,
};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, GetExitCodeProcess, INFINITE, OpenProcessToken, WaitForSingleObject,
};
use windows_sys::Win32::UI::Shell::{SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW};
use windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE;

use crate::log;
use crate::util::wide;

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const NAME: &str = "AluKeys";
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Off,
    Normal,
    Elevated,
}

fn exe() -> String {
    std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_default()
}

pub fn is_elevated() -> bool {
    unsafe {
        let mut token: HANDLE = null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return false;
        }
        let mut e: TOKEN_ELEVATION = zeroed();
        let mut len = 0u32;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            (&mut e as *mut TOKEN_ELEVATION).cast(),
            size_of::<TOKEN_ELEVATION>() as u32,
            &mut len,
        );
        CloseHandle(token);
        ok != 0 && e.TokenIsElevated != 0
    }
}

fn run_key_set() -> bool {
    let key = wide(RUN_KEY);
    let name = wide(NAME);
    let mut size = 0u32;
    let rc = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            name.as_ptr(),
            RRF_RT_REG_SZ,
            null_mut(),
            null_mut(),
            &mut size,
        )
    };
    rc == 0
}

fn set_run_key(on: bool) {
    unsafe {
        let mut hkey: HKEY = null_mut();
        let key = wide(RUN_KEY);
        if RegOpenKeyExW(HKEY_CURRENT_USER, key.as_ptr(), 0, KEY_SET_VALUE, &mut hkey) != 0 {
            return;
        }
        let name = wide(NAME);
        if on {
            let value = wide(&format!("\"{}\" --autostart", exe()));
            RegSetValueExW(hkey, name.as_ptr(), 0, REG_SZ, value.as_ptr().cast(), (value.len() * 2) as u32);
        } else {
            RegDeleteValueW(hkey, name.as_ptr());
        }
        RegCloseKey(hkey);
    }
}

fn task_exists() -> bool {
    Command::new("schtasks")
        .args(["/Query", "/TN", NAME])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

pub fn current() -> Mode {
    if task_exists() {
        Mode::Elevated
    } else if run_key_set() {
        Mode::Normal
    } else {
        Mode::Off
    }
}

/// Runs a program elevated (one UAC prompt) and waits for it. Returns its exit code.
fn run_elevated(file: &str, params: &str) -> Option<u32> {
    let verb = wide("runas");
    let file = wide(file);
    let params = wide(params);
    unsafe {
        let mut sei: SHELLEXECUTEINFOW = zeroed();
        sei.cbSize = size_of::<SHELLEXECUTEINFOW>() as u32;
        sei.fMask = SEE_MASK_NOCLOSEPROCESS;
        sei.lpVerb = verb.as_ptr();
        sei.lpFile = file.as_ptr();
        sei.lpParameters = params.as_ptr();
        sei.nShow = SW_HIDE;
        if ShellExecuteExW(&mut sei) == 0 || sei.hProcess.is_null() {
            return None;
        }
        WaitForSingleObject(sei.hProcess, INFINITE);
        let mut code = 1u32;
        GetExitCodeProcess(sei.hProcess, &mut code);
        CloseHandle(sei.hProcess);
        Some(code)
    }
}

/// Starts this program again elevated; the caller should exit if it returns true.
pub fn relaunch_elevated() -> bool {
    let verb = wide("runas");
    let file = wide(&exe());
    let params = wide("--replace");
    unsafe {
        let mut sei: SHELLEXECUTEINFOW = zeroed();
        sei.cbSize = size_of::<SHELLEXECUTEINFOW>() as u32;
        sei.lpVerb = verb.as_ptr();
        sei.lpFile = file.as_ptr();
        sei.lpParameters = params.as_ptr();
        sei.nShow = SW_HIDE;
        ShellExecuteExW(&mut sei) != 0
    }
}

fn user_id() -> String {
    let domain = std::env::var("USERDOMAIN").unwrap_or_default();
    let user = std::env::var("USERNAME").unwrap_or_default();
    if domain.is_empty() { user } else { format!("{domain}\\{user}") }
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Task definition: at logon of this user, highest privileges, no time limit, also on
/// battery, normal priority (the schtasks default is below normal and a 72 h limit).
fn task_xml() -> String {
    let user = xml_escape(&user_id());
    let exe = xml_escape(&exe());
    format!(
        r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo><Description>AluKeys: Apple keyboard support</Description></RegistrationInfo>
  <Triggers><LogonTrigger><Enabled>true</Enabled><UserId>{user}</UserId></LogonTrigger></Triggers>
  <Principals><Principal id="Author"><UserId>{user}</UserId><LogonType>InteractiveToken</LogonType><RunLevel>HighestAvailable</RunLevel></Principal></Principals>
  <Settings>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <ExecutionTimeLimit>PT0S</ExecutionTimeLimit>
    <Priority>4</Priority>
    <StartWhenAvailable>true</StartWhenAvailable>
  </Settings>
  <Actions Context="Author"><Exec><Command>{exe}</Command><Arguments>--autostart</Arguments></Exec></Actions>
</Task>"#
    )
}

/// Switches the autostart mode. Elevated changes cost one UAC prompt.
pub fn set(mode: Mode) -> bool {
    let had_task = task_exists();
    let ok = match mode {
        Mode::Elevated => {
            let path = std::env::temp_dir().join("alukeys-task.xml");
            // schtasks expects UTF-16 with a BOM for this encoding declaration.
            let mut bytes = vec![0xFF, 0xFE];
            for u in task_xml().encode_utf16() {
                bytes.extend_from_slice(&u.to_le_bytes());
            }
            if std::fs::write(&path, bytes).is_err() {
                return false;
            }
            let params = format!("/Create /TN {NAME} /XML \"{}\" /F", path.display());
            let ok = run_elevated("schtasks.exe", &params) == Some(0);
            let _ = std::fs::remove_file(&path);
            if ok {
                set_run_key(false);
            }
            ok
        }
        Mode::Normal | Mode::Off => {
            let removed =
                !had_task || run_elevated("schtasks.exe", &format!("/Delete /TN {NAME} /F")) == Some(0);
            set_run_key(mode == Mode::Normal);
            removed
        }
    };
    log!("autostart: set {mode:?}: {ok}");
    ok
}
