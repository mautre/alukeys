use std::fs::OpenOptions;
use std::io::Write;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

use windows_sys::Win32::Globalization::GetUserDefaultUILanguage;

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Copies `s` into a fixed UTF-16 buffer (truncating), as Win32 structs expect.
pub fn copy_wide<const N: usize>(dst: &mut [u16; N], s: &str) {
    let w: Vec<u16> = s.encode_utf16().take(N - 1).collect();
    dst[..w.len()].copy_from_slice(&w);
    dst[w.len()] = 0;
}

pub fn from_wide(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}

/// A window handle that can cross threads (HWND is a raw pointer).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Hwnd(pub usize);

impl Hwnd {
    pub fn get(self) -> windows_sys::Win32::Foundation::HWND {
        self.0 as _
    }
}

/// Windows build number (22000+ is Windows 11), read from the registry because
/// version APIs lie to unmanifested programs.
pub fn windows_build() -> u32 {
    use windows_sys::Win32::System::Registry::{HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ, RegGetValueW};
    static BUILD: OnceLock<u32> = OnceLock::new();
    *BUILD.get_or_init(|| {
        let key = wide(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion");
        let name = wide("CurrentBuildNumber");
        let mut buf = [0u16; 32];
        let mut size = (buf.len() * 2) as u32;
        let rc = unsafe {
            RegGetValueW(
                HKEY_LOCAL_MACHINE,
                key.as_ptr(),
                name.as_ptr(),
                RRF_RT_REG_SZ,
                std::ptr::null_mut(),
                buf.as_mut_ptr().cast(),
                &mut size,
            )
        };
        if rc != 0 {
            return 0;
        }
        from_wide(&buf).parse().unwrap_or(0)
    })
}

/// Whether apps use the dark theme (Settings → Personalisation → Colours).
pub fn apps_dark() -> bool {
    use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};
    let key = wide(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize");
    let name = wide("AppsUseLightTheme");
    let mut v = 1u32;
    let mut size = 4u32;
    let rc = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            name.as_ptr(),
            RRF_RT_REG_DWORD,
            std::ptr::null_mut(),
            (&mut v as *mut u32).cast(),
            &mut size,
        )
    };
    rc == 0 && v == 0
}

/// The application icon embedded by build.rs (resource 1) at the given pixel size.
pub fn app_icon(size: i32) -> windows_sys::Win32::UI::WindowsAndMessaging::HICON {
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{IMAGE_ICON, LR_DEFAULTCOLOR, LoadImageW};
    unsafe {
        LoadImageW(GetModuleHandleW(std::ptr::null()), 1 as _, IMAGE_ICON, size, size, LR_DEFAULTCOLOR) as _
    }
}

static RUSSIAN: AtomicBool = AtomicBool::new(false);

/// Picks the UI language: "ru", "en" or "auto" (Windows display language).
pub fn set_language(lang: &str) {
    let ru = match lang {
        "ru" => true,
        "en" => false,
        _ => (unsafe { GetUserDefaultUILanguage() } & 0x3FF) == 0x19,
    };
    RUSSIAN.store(ru, Ordering::Relaxed);
}

/// Returns the Russian or English string depending on the UI language.
pub fn tr(ru: &'static str, en: &'static str) -> &'static str {
    if RUSSIAN.load(Ordering::Relaxed) { ru } else { en }
}

static LOG_PATH: OnceLock<std::path::PathBuf> = OnceLock::new();

pub fn init_log(path: std::path::PathBuf) {
    // Keep the log small: start over when it grows past 1 MB.
    if std::fs::metadata(&path).map(|m| m.len() > 1 << 20).unwrap_or(false) {
        let _ = std::fs::remove_file(&path);
    }
    let _ = LOG_PATH.set(path);
}

pub fn log(msg: &str) {
    let Some(path) = LOG_PATH.get() else { return };
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(path) {
        let t = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let _ = writeln!(f, "{t} {msg}");
    }
}

#[macro_export]
macro_rules! log {
    ($($arg:tt)*) => { $crate::util::log(&format!($($arg)*)) };
}
