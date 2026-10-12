use std::mem::size_of;
use std::ptr::null_mut;

use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::Registry::HKEY_LOCAL_MACHINE;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetKeyboardLayoutList;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    SPI_GETKEYBOARDDELAY, SPI_GETKEYBOARDSPEED, SystemParametersInfoW,
};

use crate::util::{RegKey, from_wide};
use crate::{out, rep};

const SUSPICIOUS: &[&str] = &[
    "apple",
    "magic",
    "keymagic",
    "bootcamp",
    "interception",
    "keyboard",
    "kbdfltr",
    "sharpkeys",
    "keyboardmanager",
    "autohotkey",
    "karabiner",
];

pub fn dump() {
    if let Some(k) = RegKey::open(HKEY_LOCAL_MACHINE, r"SOFTWARE\Microsoft\Windows NT\CurrentVersion") {
        out!(
            "Windows: {} {} build {}.{}",
            k.value("ProductName").unwrap_or_default(),
            k.value("DisplayVersion").unwrap_or_default(),
            k.value("CurrentBuild").unwrap_or_default(),
            k.value("UBR").unwrap_or_default()
        );
    }
    out!("Arch: {}", std::env::consts::ARCH);

    // Keyboard layouts (HKL low word = language, high word = layout).
    let mut hkls = [null_mut(); 32];
    let n = unsafe { GetKeyboardLayoutList(hkls.len() as i32, hkls.as_mut_ptr()) };
    let list: Vec<String> =
        hkls[..n.max(0) as usize].iter().map(|h| format!("{:08X}", *h as usize as u32)).collect();
    out!("Keyboard layouts (HKL): {}", list.join(", "));

    let mut delay = 0u32;
    let mut speed = 0u32;
    unsafe {
        SystemParametersInfoW(SPI_GETKEYBOARDDELAY, 0, (&mut delay as *mut u32).cast(), 0);
        SystemParametersInfoW(SPI_GETKEYBOARDSPEED, 0, (&mut speed as *mut u32).cast(), 0);
    }
    out!("Key repeat: delay={delay} (0..3), speed={speed} (0..31)");

    // Keyboard class filter drivers: anything besides kbdclass is a potential conflict.
    let kbd_class = r"SYSTEM\CurrentControlSet\Control\Class\{4d36e96b-e325-11ce-bfc1-08002be10318}";
    if let Some(k) = RegKey::open(HKEY_LOCAL_MACHINE, kbd_class) {
        out!("Keyboard class UpperFilters: {}", k.value("UpperFilters").unwrap_or("-".into()));
        out!("Keyboard class LowerFilters: {}", k.value("LowerFilters").unwrap_or("-".into()));
    }
    let hid_class = r"SYSTEM\CurrentControlSet\Control\Class\{745a17a0-74d3-11d0-b6fe-00a0c90f57da}";
    if let Some(k) = RegKey::open(HKEY_LOCAL_MACHINE, hid_class) {
        out!("HID class UpperFilters: {}", k.value("UpperFilters").unwrap_or("-".into()));
        out!("HID class LowerFilters: {}", k.value("LowerFilters").unwrap_or("-".into()));
    }
    if let Some(k) = RegKey::open(HKEY_LOCAL_MACHINE, r"SYSTEM\CurrentControlSet\Control\Keyboard Layout") {
        out!("Scancode Map: {}", k.value("Scancode Map").unwrap_or("-".into()));
    }

    // Services whose names hint at keyboard remappers or Apple drivers.
    if let Some(k) = RegKey::open(HKEY_LOCAL_MACHINE, r"SYSTEM\CurrentControlSet\Services") {
        let hits: Vec<String> = k
            .subkeys()
            .into_iter()
            .filter(|s| {
                let l = s.to_ascii_lowercase();
                SUSPICIOUS.iter().any(|p| l.contains(p))
            })
            .collect();
        out!("Services of interest: {}", if hits.is_empty() { "-".into() } else { hits.join(", ") });
        for s in &hits {
            if let Some(sk) =
                RegKey::open(HKEY_LOCAL_MACHINE, &format!(r"SYSTEM\CurrentControlSet\Services\{s}"))
            {
                rep!(
                    "  {s}: ImagePath={} Start={} DisplayName={}",
                    sk.value("ImagePath").unwrap_or_default(),
                    sk.value("Start").unwrap_or_default(),
                    sk.value("DisplayName").unwrap_or_default()
                );
            }
        }
    }

    out!("Processes of interest: {}", processes_of_interest().join(", "));
}

fn processes_of_interest() -> Vec<String> {
    let mut hits = Vec::new();
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap == INVALID_HANDLE_VALUE {
            return hits;
        }
        let mut e: PROCESSENTRY32W = std::mem::zeroed();
        e.dwSize = size_of::<PROCESSENTRY32W>() as u32;
        let mut ok = Process32FirstW(snap, &mut e) != 0;
        while ok {
            let name = from_wide(&e.szExeFile);
            let l = name.to_ascii_lowercase();
            if SUSPICIOUS.iter().any(|p| l.contains(p)) || l.contains("powertoys") {
                hits.push(name);
            }
            ok = Process32NextW(snap, &mut e) != 0;
        }
        CloseHandle(snap);
    }
    hits.sort();
    hits.dedup();
    if hits.is_empty() {
        hits.push("-".into());
    }
    hits
}
