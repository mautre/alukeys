//! On-screen indicator in the style of the Windows 11 volume flyout: a rounded pill at
//! the bottom centre of the active monitor that fades out on its own. It never takes
//! focus or mouse input.

use std::cell::RefCell;
use std::mem::zeroed;
use std::ptr::null_mut;

use windows_sys::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Dwm::{DWMWA_WINDOW_CORNER_PREFERENCE, DwmSetWindowAttribute};
use windows_sys::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, CLEARTYPE_QUALITY, CreateCompatibleBitmap, CreateCompatibleDC, CreateFontW,
    CreateSolidBrush, DT_CENTER, DT_LEFT, DT_SINGLELINE, DT_VCENTER, DeleteDC, DeleteObject, DrawTextW,
    EndPaint, FW_NORMAL, FW_SEMIBOLD, FillRect, GetMonitorInfoW, GetStockObject, HGDIOBJ, InvalidateRect,
    MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint, NULL_PEN, PAINTSTRUCT, RoundRect, SRCCOPY,
    SelectObject, SetBkMode, SetTextColor, TRANSPARENT,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Registry::HKEY_CURRENT_USER;
use windows_sys::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, GetCursorPos, HWND_TOPMOST, KillTimer, LWA_ALPHA, RegisterClassW,
    SW_HIDE, SWP_NOACTIVATE, SWP_SHOWWINDOW, SetLayeredWindowAttributes, SetTimer, SetWindowPos, ShowWindow,
    WM_PAINT, WM_TIMER, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
    WS_EX_TRANSPARENT, WS_POPUP,
};

use crate::util::wide;

pub mod glyph {
    pub const BRIGHTNESS: char = '\u{E706}';
    pub const KEYBOARD: char = '\u{E765}';
    pub const PAUSE: char = '\u{E769}';
    pub const PLAY: char = '\u{E768}';
    pub const WARNING: char = '\u{E7BA}';
}

const TIMER_FADE: usize = 1;
const VISIBLE_MS: u32 = 1300;
const FADE_STEP_MS: u32 = 15;
const ALPHA: u8 = 245;

struct Content {
    glyph: char,
    text: String,
    level: Option<u32>,
}

struct Osd {
    hwnd: HWND,
    content: Content,
    alpha: u8,
    fading: bool,
    scale: f32,
}

thread_local! {
    static OSD: RefCell<Option<Osd>> = const { RefCell::new(None) };
}

fn dark_mode() -> bool {
    use windows_sys::Win32::System::Registry::{RRF_RT_REG_DWORD, RegGetValueW};
    let key = wide(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize");
    let name = wide("SystemUsesLightTheme");
    let mut v = 1u32;
    let mut size = 4u32;
    let rc = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            name.as_ptr(),
            RRF_RT_REG_DWORD,
            null_mut(),
            (&mut v as *mut u32).cast(),
            &mut size,
        )
    };
    rc == 0 && v == 0
}

fn rgb(r: u8, g: u8, b: u8) -> COLORREF {
    r as u32 | (g as u32) << 8 | (b as u32) << 16
}

pub fn init() {
    unsafe {
        let hinst = GetModuleHandleW(null_mut());
        let class = wide("AluKeysOsd");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wnd_proc),
            hInstance: hinst,
            lpszClassName: class.as_ptr(),
            ..zeroed()
        };
        RegisterClassW(&wc);
        let hwnd = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT,
            class.as_ptr(),
            class.as_ptr(),
            WS_POPUP,
            0,
            0,
            0,
            0,
            null_mut(),
            null_mut(),
            hinst,
            null_mut(),
        );
        // Windows 11 rounds the corners of the pill.
        let round: u32 = 2; // DWMWCP_ROUND
        DwmSetWindowAttribute(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE as u32, (&round as *const u32).cast(), 4);
        OSD.with(|o| {
            *o.borrow_mut() = Some(Osd {
                hwnd,
                content: Content { glyph: ' ', text: String::new(), level: None },
                alpha: 0,
                fading: false,
                scale: 1.0,
            })
        });
    }
}

/// Shows the indicator with an icon, a label and an optional 0–100 level bar.
pub fn show(glyph: char, text: &str, level: Option<u32>) {
    OSD.with(|o| {
        let mut o = o.borrow_mut();
        let Some(o) = o.as_mut() else { return };
        o.content = Content { glyph, text: text.to_string(), level };
        unsafe {
            let mut pt = POINT { x: 0, y: 0 };
            GetCursorPos(&mut pt);
            let mon = MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST);
            let mut mi: MONITORINFO = zeroed();
            mi.cbSize = size_of::<MONITORINFO>() as u32;
            GetMonitorInfoW(mon, &mut mi);
            let (mut dx, mut dy) = (96u32, 96u32);
            GetDpiForMonitor(mon, MDT_EFFECTIVE_DPI, &mut dx, &mut dy);
            o.scale = dx as f32 / 96.0;
            let w = (if level.is_some() { 280.0 } else { 240.0 } * o.scale) as i32;
            let h = (48.0 * o.scale) as i32;
            let work = mi.rcWork;
            let x = work.left + (work.right - work.left - w) / 2;
            let y = work.bottom - h - (24.0 * o.scale) as i32;
            o.alpha = ALPHA;
            o.fading = false;
            SetLayeredWindowAttributes(o.hwnd, 0, o.alpha, LWA_ALPHA);
            SetWindowPos(o.hwnd, HWND_TOPMOST, x, y, w, h, SWP_NOACTIVATE | SWP_SHOWWINDOW);
            InvalidateRect(o.hwnd, null_mut(), 0);
            SetTimer(o.hwnd, TIMER_FADE, VISIBLE_MS, None);
        }
    });
}

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_PAINT => {
            OSD.with(|o| {
                if let Some(o) = o.borrow().as_ref() {
                    unsafe { paint(o) };
                }
            });
            0
        }
        WM_TIMER if wparam == TIMER_FADE => {
            OSD.with(|o| {
                let mut o = o.borrow_mut();
                let Some(o) = o.as_mut() else { return };
                unsafe {
                    if !o.fading {
                        o.fading = true;
                        SetTimer(hwnd, TIMER_FADE, FADE_STEP_MS, None);
                    }
                    o.alpha = o.alpha.saturating_sub(24);
                    if o.alpha == 0 {
                        KillTimer(hwnd, TIMER_FADE);
                        ShowWindow(hwnd, SW_HIDE);
                    } else {
                        SetLayeredWindowAttributes(hwnd, 0, o.alpha, LWA_ALPHA);
                    }
                }
            });
            0
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

unsafe fn font(name: &str, px: i32, weight: i32) -> HGDIOBJ {
    let n = wide(name);
    unsafe { CreateFontW(-px, 0, 0, 0, weight, 0, 0, 0, 1, 0, 0, CLEARTYPE_QUALITY as u32, 0, n.as_ptr()) }
}

unsafe fn paint(o: &Osd) {
    let dark = dark_mode();
    let (bg, fg, track, fill) = if dark {
        (rgb(0x2C, 0x2C, 0x2C), rgb(0xFF, 0xFF, 0xFF), rgb(0x5A, 0x5A, 0x5A), rgb(0x4C, 0xC2, 0xFF))
    } else {
        (rgb(0xF9, 0xF9, 0xF9), rgb(0x1A, 0x1A, 0x1A), rgb(0xC8, 0xC8, 0xC8), rgb(0x00, 0x5F, 0xB8))
    };
    let s = |v: f32| (v * o.scale) as i32;
    unsafe {
        let mut ps: PAINTSTRUCT = zeroed();
        let hdc = BeginPaint(o.hwnd, &mut ps);
        let mut rc: RECT = zeroed();
        windows_sys::Win32::UI::WindowsAndMessaging::GetClientRect(o.hwnd, &mut rc);
        let mem = CreateCompatibleDC(hdc);
        let bmp = CreateCompatibleBitmap(hdc, rc.right, rc.bottom);
        let old_bmp = SelectObject(mem, bmp);

        let bg_brush = CreateSolidBrush(bg);
        FillRect(mem, &rc, bg_brush);
        DeleteObject(bg_brush);
        SetBkMode(mem, TRANSPARENT as i32);
        SetTextColor(mem, fg);

        // Icon.
        let icon_font = font(
            if crate::util::windows_build() >= 22000 { "Segoe Fluent Icons" } else { "Segoe MDL2 Assets" },
            s(18.0),
            FW_NORMAL as i32,
        );
        let old_font = SelectObject(mem, icon_font);
        let mut g: Vec<u16> = o.content.glyph.to_string().encode_utf16().collect();
        let mut icon_rc = RECT { left: s(14.0), top: 0, right: s(44.0), bottom: rc.bottom };
        DrawTextW(mem, g.as_mut_ptr(), g.len() as i32, &mut icon_rc, DT_CENTER | DT_VCENTER | DT_SINGLELINE);

        // Label.
        let text_font = font("Segoe UI Variable Text", s(14.0), FW_SEMIBOLD as i32);
        SelectObject(mem, text_font);
        let mut t: Vec<u16> = o.content.text.encode_utf16().collect();
        let text_right = if o.content.level.is_some() { s(96.0) } else { rc.right - s(14.0) };
        let mut text_rc = RECT { left: s(52.0), top: 0, right: text_right, bottom: rc.bottom };
        DrawTextW(mem, t.as_mut_ptr(), t.len() as i32, &mut text_rc, DT_LEFT | DT_VCENTER | DT_SINGLELINE);

        // Level bar.
        if let Some(level) = o.content.level {
            let left = s(104.0);
            let right = rc.right - s(20.0);
            let cy = rc.bottom / 2;
            let h = s(4.0).max(2);
            let pen = SelectObject(mem, GetStockObject(NULL_PEN));
            let track_brush = CreateSolidBrush(track);
            let old_brush = SelectObject(mem, track_brush);
            RoundRect(mem, left, cy - h / 2, right, cy + h / 2 + 1, h, h);
            let fill_brush = CreateSolidBrush(fill);
            SelectObject(mem, fill_brush);
            let x = left + (right - left) * level.min(100) as i32 / 100;
            if x > left {
                RoundRect(mem, left, cy - h / 2, x, cy + h / 2 + 1, h, h);
            }
            // Thumb.
            let r = s(7.0);
            RoundRect(mem, x - r, cy - r, x + r, cy + r, r * 2, r * 2);
            SelectObject(mem, old_brush);
            SelectObject(mem, pen);
            DeleteObject(track_brush);
            DeleteObject(fill_brush);
        }

        BitBlt(hdc, 0, 0, rc.right, rc.bottom, mem, 0, 0, SRCCOPY);
        SelectObject(mem, old_font);
        DeleteObject(icon_font);
        DeleteObject(text_font);
        SelectObject(mem, old_bmp);
        DeleteObject(bmp);
        DeleteDC(mem);
        EndPaint(o.hwnd, &ps);
    }
}
