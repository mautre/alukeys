//! Launch splash: the app icon, name and status in a rounded card at the centre of the
//! screen that fades in, stays for a moment and fades out. Click-through, never focused.

use std::cell::RefCell;
use std::mem::zeroed;
use std::ptr::null_mut;

use windows_sys::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Dwm::{DWMWA_WINDOW_CORNER_PREFERENCE, DwmSetWindowAttribute};
use windows_sys::Win32::Graphics::Gdi::{
    BeginPaint, CLEARTYPE_QUALITY, CreateFontW, CreateSolidBrush, DT_CENTER, DT_SINGLELINE, DT_TOP,
    DeleteObject, DrawTextW, EndPaint, FW_NORMAL, FW_SEMIBOLD, FillRect, GetMonitorInfoW,
    MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint, PAINTSTRUCT, SelectObject, SetBkMode,
    SetTextColor, TRANSPARENT,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DI_NORMAL, DefWindowProcW, DestroyIcon, DestroyWindow, DrawIconEx, GetClientRect,
    GetCursorPos, HICON, HWND_TOPMOST, KillTimer, LWA_ALPHA, RegisterClassW, SWP_NOACTIVATE, SWP_SHOWWINDOW,
    SetLayeredWindowAttributes, SetTimer, SetWindowPos, WM_PAINT, WM_TIMER, WNDCLASSW, WS_EX_LAYERED,
    WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};

use crate::util::{app_icon, apps_dark, wide};

const TIMER: usize = 1;
const STEP_MS: u32 = 15;
const FADE_IN_MS: u32 = 180;
const HOLD_MS: u32 = 1100;
const FADE_OUT_MS: u32 = 450;
const ALPHA: f32 = 248.0;

struct Splash {
    title: String,
    status: String,
    icon: HICON,
    scale: f32,
    elapsed: u32,
}

thread_local! {
    static SPLASH: RefCell<Option<Splash>> = const { RefCell::new(None) };
}

fn rgb(r: u8, g: u8, b: u8) -> COLORREF {
    r as u32 | (g as u32) << 8 | (b as u32) << 16
}

/// Shows the splash with a status line; it removes itself when done.
pub fn show(status: &str) {
    unsafe {
        let hinst = GetModuleHandleW(null_mut());
        let class = wide("AluKeysSplash");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wnd_proc),
            hInstance: hinst,
            lpszClassName: class.as_ptr(),
            ..zeroed()
        };
        RegisterClassW(&wc);

        let mut pt = POINT { x: 0, y: 0 };
        GetCursorPos(&mut pt);
        let mon = MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST);
        let mut mi: MONITORINFO = zeroed();
        mi.cbSize = size_of::<MONITORINFO>() as u32;
        GetMonitorInfoW(mon, &mut mi);
        let (mut dx, mut dy) = (96u32, 96u32);
        GetDpiForMonitor(mon, MDT_EFFECTIVE_DPI, &mut dx, &mut dy);
        let scale = dx as f32 / 96.0;
        let (w, h) = ((300.0 * scale) as i32, (196.0 * scale) as i32);
        let work = mi.rcWork;
        let x = work.left + (work.right - work.left - w) / 2;
        let y = work.top + (work.bottom - work.top - h) / 2;

        let hwnd = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT,
            class.as_ptr(),
            class.as_ptr(),
            WS_POPUP,
            x,
            y,
            w,
            h,
            null_mut(),
            null_mut(),
            hinst,
            null_mut(),
        );
        if hwnd.is_null() {
            return;
        }
        let round: u32 = 2; // DWMWCP_ROUND
        DwmSetWindowAttribute(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE as u32, (&round as *const u32).cast(), 4);
        SPLASH.with(|s| {
            *s.borrow_mut() = Some(Splash {
                title: "AluKeys".into(),
                status: status.to_string(),
                icon: app_icon((64.0 * scale) as i32),
                scale,
                elapsed: 0,
            })
        });
        SetLayeredWindowAttributes(hwnd, 0, 0, LWA_ALPHA);
        SetWindowPos(hwnd, HWND_TOPMOST, x, y, w, h, SWP_NOACTIVATE | SWP_SHOWWINDOW);
        SetTimer(hwnd, TIMER, STEP_MS, None);
    }
}

/// Opacity for the time since the splash appeared (ease in, hold, ease out).
fn alpha_at(t: u32) -> Option<u8> {
    let a = if t < FADE_IN_MS {
        let p = t as f32 / FADE_IN_MS as f32;
        1.0 - (1.0 - p).powi(3)
    } else if t < FADE_IN_MS + HOLD_MS {
        1.0
    } else if t < FADE_IN_MS + HOLD_MS + FADE_OUT_MS {
        let p = (t - FADE_IN_MS - HOLD_MS) as f32 / FADE_OUT_MS as f32;
        1.0 - p * p
    } else {
        return None;
    };
    Some((a * ALPHA) as u8)
}

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_TIMER if wparam == TIMER => {
            let next = SPLASH.with(|s| {
                let mut s = s.borrow_mut();
                let s = s.as_mut()?;
                s.elapsed += STEP_MS;
                alpha_at(s.elapsed)
            });
            unsafe {
                match next {
                    Some(a) => {
                        SetLayeredWindowAttributes(hwnd, 0, a, LWA_ALPHA);
                    }
                    None => {
                        KillTimer(hwnd, TIMER);
                        DestroyWindow(hwnd);
                        if let Some(s) = SPLASH.with(|s| s.borrow_mut().take()) {
                            DestroyIcon(s.icon);
                        }
                    }
                }
            }
            0
        }
        WM_PAINT => {
            SPLASH.with(|s| {
                if let Some(s) = s.borrow().as_ref() {
                    unsafe { paint(hwnd, s) };
                }
            });
            0
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

unsafe fn paint(hwnd: HWND, s: &Splash) {
    let dark = apps_dark();
    let (bg, fg, fg2) = if dark {
        (rgb(0x2B, 0x2B, 0x2B), rgb(0xFF, 0xFF, 0xFF), rgb(0xC8, 0xC8, 0xC8))
    } else {
        (rgb(0xFB, 0xFB, 0xFB), rgb(0x1A, 0x1A, 0x1A), rgb(0x5C, 0x5C, 0x5C))
    };
    let px = |v: f32| (v * s.scale) as i32;
    unsafe {
        let mut ps: PAINTSTRUCT = zeroed();
        let hdc = BeginPaint(hwnd, &mut ps);
        let mut rc: RECT = zeroed();
        GetClientRect(hwnd, &mut rc);
        let brush = CreateSolidBrush(bg);
        FillRect(hdc, &rc, brush);
        DeleteObject(brush);

        let icon = px(64.0);
        DrawIconEx(hdc, (rc.right - icon) / 2, px(28.0), s.icon, icon, icon, 0, null_mut(), DI_NORMAL);

        SetBkMode(hdc, TRANSPARENT as i32);
        let face = wide("Segoe UI Variable Display");
        let title_font = CreateFontW(
            -px(24.0),
            0,
            0,
            0,
            FW_SEMIBOLD as i32,
            0,
            0,
            0,
            1,
            0,
            0,
            CLEARTYPE_QUALITY as u32,
            0,
            face.as_ptr(),
        );
        let old = SelectObject(hdc, title_font);
        SetTextColor(hdc, fg);
        let mut t: Vec<u16> = s.title.encode_utf16().collect();
        let mut r = RECT { left: 0, top: px(104.0), right: rc.right, bottom: px(136.0) };
        DrawTextW(hdc, t.as_mut_ptr(), t.len() as i32, &mut r, DT_CENTER | DT_TOP | DT_SINGLELINE);

        let face = wide("Segoe UI Variable Text");
        let status_font = CreateFontW(
            -px(13.0),
            0,
            0,
            0,
            FW_NORMAL as i32,
            0,
            0,
            0,
            1,
            0,
            0,
            CLEARTYPE_QUALITY as u32,
            0,
            face.as_ptr(),
        );
        SelectObject(hdc, status_font);
        SetTextColor(hdc, fg2);
        let mut st: Vec<u16> = s.status.encode_utf16().collect();
        let mut r = RECT { left: px(12.0), top: px(142.0), right: rc.right - px(12.0), bottom: px(170.0) };
        DrawTextW(hdc, st.as_mut_ptr(), st.len() as i32, &mut r, DT_CENTER | DT_TOP | DT_SINGLELINE);

        SelectObject(hdc, old);
        DeleteObject(title_font);
        DeleteObject(status_font);
        EndPaint(hwnd, &ps);
    }
}
