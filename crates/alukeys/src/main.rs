//! AluKeys: Apple keyboard support for Windows.
#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
mod app;
#[cfg(windows)]
mod autostart;
#[cfg(windows)]
mod devices;
#[cfg(windows)]
mod engine_thread;
#[cfg(windows)]
mod inject;
#[cfg(windows)]
mod osd;
#[cfg(windows)]
mod settings;
#[cfg(windows)]
mod settings_window;
#[cfg(windows)]
mod shared;
#[cfg(windows)]
mod tray;
#[cfg(windows)]
mod ui_state;
#[cfg(windows)]
mod util;
#[cfg(windows)]
mod worker;

#[cfg(windows)]
fn main() {
    use windows_sys::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError};
    use windows_sys::Win32::System::Threading::CreateMutexW;
    use windows_sys::Win32::UI::HiDpi::{
        DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{FindWindowW, PostMessageW, WM_CLOSE};

    let replace = std::env::args().any(|a| a == "--replace");
    util::init_log(settings::log_path());
    std::panic::set_hook(Box::new(|info| util::log(&format!("PANIC: {info}"))));
    log!("start {} elevated={}", env!("CARGO_PKG_VERSION"), autostart::is_elevated());

    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);

        let name = util::wide("Local\\AluKeys.Instance");
        CreateMutexW(std::ptr::null(), 0, name.as_ptr());
        if GetLastError() == ERROR_ALREADY_EXISTS {
            let class = util::wide("AluKeysTray");
            let other = FindWindowW(class.as_ptr(), std::ptr::null());
            if replace {
                // Take over from a running copy (e.g. restarting as administrator).
                if !other.is_null() {
                    PostMessageW(other, WM_CLOSE, 0, 0);
                }
                let gone = (0..50).any(|_| {
                    std::thread::sleep(std::time::Duration::from_millis(100));
                    FindWindowW(class.as_ptr(), std::ptr::null()).is_null()
                });
                if !gone {
                    return;
                }
            } else {
                if !other.is_null() {
                    PostMessageW(other, app::WM_SHOW_MENU, 0, 0);
                }
                return;
            }
        }
    }

    let (config, error) = settings::load();
    util::set_language(&config.ui.language);
    app::run(config, error);
    log!("exit");
}

#[cfg(not(windows))]
fn main() {
    eprintln!("AluKeys runs on Windows only.");
    std::process::exit(1);
}
