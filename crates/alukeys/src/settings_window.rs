//! The settings window: a WebView2 page (embedded HTML) in a normal Win32 window,
//! created on demand on the UI thread and destroyed on close, so it costs nothing while
//! closed.
//!
//! Page → app: `window.ipc.postMessage(json)`; queued and handled by the tray window
//! (`WM_SETTINGS_IPC`) so handlers never run inside WebView2 callbacks.
//! App → page: `window.__recv(json)`.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::mem::zeroed;
use std::num::NonZeroIsize;
use std::ptr::null_mut;
use std::sync::Mutex;

use raw_window_handle::{HandleError, HasWindowHandle, RawWindowHandle, Win32WindowHandle, WindowHandle};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Dwm::{DWMWA_USE_IMMERSIVE_DARK_MODE, DwmSetWindowAttribute};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
use windows_sys::Win32::UI::WindowsAndMessaging::*;
use wry::{WebContext, WebView, WebViewBuilder, WebViewBuilderExtWindows};

use crate::shared::shared;
use crate::util::wide;

/// Page messages waiting for the tray window.
pub const WM_SETTINGS_IPC: u32 = WM_APP + 40;

static INBOX: Mutex<VecDeque<String>> = Mutex::new(VecDeque::new());

const PAGE: &str = include_str!("../ui/settings.html");

struct Window {
    hwnd: HWND,
    webview: WebView,
    _context: WebContext,
}

thread_local! {
    static WINDOW: RefCell<Option<Window>> = const { RefCell::new(None) };
}

struct Handle(HWND);

impl HasWindowHandle for Handle {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        let h = Win32WindowHandle::new(NonZeroIsize::new(self.0 as isize).ok_or(HandleError::Unavailable)?);
        Ok(unsafe { WindowHandle::borrow_raw(RawWindowHandle::Win32(h)) })
    }
}

pub fn is_open() -> bool {
    WINDOW.with(|w| w.borrow().is_some())
}

pub fn take_messages() -> Vec<String> {
    INBOX.lock().map(|mut q| q.drain(..).collect()).unwrap_or_default()
}

/// Sends a JSON message to the page.
pub fn send(json: &str) {
    WINDOW.with(|w| {
        if let Some(w) = w.borrow().as_ref() {
            let _ = w.webview.evaluate_script(&format!("window.__recv && window.__recv({json})"));
        }
    });
}

/// Opens the window, or brings it to the front. `page` selects the start page.
pub fn open(page: &str, dark: bool) {
    let existing = WINDOW.with(|w| w.borrow().as_ref().map(|w| w.hwnd));
    if let Some(hwnd) = existing {
        unsafe {
            ShowWindow(hwnd, SW_RESTORE);
            SetForegroundWindow(hwnd);
        }
        return;
    }
    unsafe {
        let hinst = GetModuleHandleW(null_mut());
        let dpi = windows_sys::Win32::UI::HiDpi::GetDpiForSystem();
        let metric = |m| windows_sys::Win32::UI::HiDpi::GetSystemMetricsForDpi(m, dpi);
        let icon = crate::util::app_icon(metric(SM_CXICON));
        let small = crate::util::app_icon(metric(SM_CXSMICON));
        let class = wide("AluKeysSettings");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wnd_proc),
            hInstance: hinst,
            lpszClassName: class.as_ptr(),
            hCursor: LoadCursorW(null_mut(), IDC_ARROW),
            hIcon: icon,
            ..zeroed()
        };
        RegisterClassW(&wc);
        let title = wide(crate::util::tr("AluKeys — настройки", "AluKeys Settings"));
        let hwnd = CreateWindowExW(
            0,
            class.as_ptr(),
            title.as_ptr(),
            WS_OVERLAPPEDWINDOW,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            100,
            100,
            null_mut(),
            null_mut(),
            hinst,
            null_mut(),
        );
        if hwnd.is_null() {
            return;
        }
        let on: u32 = dark as u32;
        DwmSetWindowAttribute(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE as u32, (&on as *const u32).cast(), 4);
        SendMessageW(hwnd, WM_SETICON, ICON_BIG as usize, icon as isize);
        SendMessageW(hwnd, WM_SETICON, ICON_SMALL as usize, small as isize);

        // Size for the monitor's DPI and centre on the work area.
        let scale = GetDpiForWindow(hwnd) as f32 / 96.0;
        let (w, h) = ((1120.0 * scale) as i32, (760.0 * scale) as i32);
        let mut work: RECT = zeroed();
        SystemParametersInfoW(SPI_GETWORKAREA, 0, (&mut work as *mut RECT).cast(), 0);
        let w = w.min(work.right - work.left);
        let h = h.min(work.bottom - work.top);
        let x = work.left + (work.right - work.left - w) / 2;
        let y = work.top + (work.bottom - work.top - h) / 2;
        SetWindowPos(hwnd, null_mut(), x, y, w, h, SWP_NOZORDER);

        let mut context = WebContext::new(Some(crate::settings::data_dir().join("webview")));
        let ui = shared().ui;
        let bg = if dark { (32, 32, 32, 255) } else { (243, 243, 243, 255) };
        let html = PAGE.replace("__START_PAGE__", page);
        let built = WebViewBuilder::new_with_web_context(&mut context)
            .with_html(html)
            .with_background_color(bg)
            .with_devtools(cfg!(debug_assertions))
            // No reload, print or find from browser shortcuts: keys belong to the app.
            .with_browser_accelerator_keys(false)
            .with_focused(true)
            .with_ipc_handler(move |req| {
                if let Ok(mut q) = INBOX.lock() {
                    q.push_back(req.body().clone());
                }
                PostMessageW(ui.get(), WM_SETTINGS_IPC, 0, 0);
            })
            .build(&Handle(hwnd));
        match built {
            Ok(webview) => {
                WINDOW.with(|w| *w.borrow_mut() = Some(Window { hwnd, webview, _context: context }));
                ShowWindow(hwnd, SW_SHOWNORMAL);
                SetForegroundWindow(hwnd);
            }
            Err(e) => {
                crate::log!("settings: webview failed: {e}");
                DestroyWindow(hwnd);
                let text = wide(crate::util::tr(
                    "Не удалось открыть окно настроек: не найден Microsoft Edge WebView2.\nУстановите WebView2 Runtime с сайта Microsoft или измените файл настроек вручную.",
                    "Could not open settings: Microsoft Edge WebView2 was not found.\nInstall the WebView2 Runtime from Microsoft, or edit the settings file.",
                ));
                let cap = wide("AluKeys");
                MessageBoxW(null_mut(), text.as_ptr(), cap.as_ptr(), MB_OK | MB_ICONWARNING);
            }
        }
    }
}

pub fn close() {
    let hwnd = WINDOW.with(|w| w.borrow().as_ref().map(|w| w.hwnd));
    if let Some(hwnd) = hwnd {
        unsafe { DestroyWindow(hwnd) };
    }
}

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_GETMINMAXINFO => {
            let mmi = unsafe { &mut *(lparam as *mut MINMAXINFO) };
            let scale = unsafe { GetDpiForWindow(hwnd) } as f32 / 96.0;
            mmi.ptMinTrackSize.x = (760.0 * scale) as i32;
            mmi.ptMinTrackSize.y = (520.0 * scale) as i32;
            0
        }
        WM_DESTROY => {
            // Dropping the WebView frees the browser process memory.
            WINDOW.with(|w| w.borrow_mut().take());
            let ui = shared().ui;
            unsafe { PostMessageW(ui.get(), WM_SETTINGS_CLOSED, 0, 0) };
            0
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

/// Posted to the tray window when the settings window closes.
pub const WM_SETTINGS_CLOSED: u32 = WM_APP + 41;
