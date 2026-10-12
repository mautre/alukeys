//! Notification-area icon: drawn at runtime from the system icon font, so it follows the
//! taskbar theme and DPI without image files, and shows paused/disconnected/low-battery.

use std::mem::{size_of, zeroed};
use std::ptr::null_mut;

use windows_sys::Win32::Foundation::{HWND, RECT};
use windows_sys::Win32::Graphics::Gdi::{
    ANTIALIASED_QUALITY, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateBitmap, CreateCompatibleDC,
    CreateDIBSection, CreateFontW, DIB_RGB_COLORS, DT_CENTER, DT_SINGLELINE, DT_VCENTER, DeleteDC,
    DeleteObject, DrawTextW, FW_NORMAL, SelectObject, SetBkMode, SetTextColor, TRANSPARENT,
};
use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};
use windows_sys::Win32::UI::HiDpi::{GetDpiForSystem, GetSystemMetricsForDpi};
use windows_sys::Win32::UI::Shell::{
    NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_SHOWTIP, NIF_TIP, NIIF_INFO, NIIF_WARNING, NIM_ADD, NIM_DELETE,
    NIM_MODIFY, NIM_SETVERSION, NOTIFYICON_VERSION_4, NOTIFYICONDATAW, Shell_NotifyIconW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateIconIndirect, DestroyIcon, HICON, ICONINFO, SM_CXSMICON,
};

use crate::shared::WM_TRAY;
use crate::util::{copy_wide, wide};

const ID: u32 = 1;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Look {
    Normal,
    Dimmed,
    LowBattery,
}

fn taskbar_light() -> bool {
    let key = wide(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize");
    let name = wide("SystemUsesLightTheme");
    let mut v = 0u32;
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
    rc == 0 && v == 1
}

/// Draws the keyboard glyph into an icon with real alpha.
pub fn make_icon(look: Look) -> HICON {
    unsafe {
        let dpi = GetDpiForSystem();
        let size = GetSystemMetricsForDpi(SM_CXSMICON, dpi).max(16);
        let mut bi: BITMAPINFO = zeroed();
        bi.bmiHeader = BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: size,
            biHeight: -size,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            ..zeroed()
        };
        let mut bits: *mut core::ffi::c_void = null_mut();
        let dc = CreateCompatibleDC(null_mut());
        let bmp = CreateDIBSection(dc, &bi, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
        let old = SelectObject(dc, bmp);

        // White glyph on black: the red channel becomes the coverage mask.
        let face =
            if crate::util::windows_build() >= 22000 { "Segoe Fluent Icons" } else { "Segoe MDL2 Assets" };
        let f = wide(face);
        let font = CreateFontW(
            -size,
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
            ANTIALIASED_QUALITY as u32,
            0,
            f.as_ptr(),
        );
        let old_font = SelectObject(dc, font);
        SetBkMode(dc, TRANSPARENT as i32);
        SetTextColor(dc, 0x00FF_FFFF);
        let mut glyph: Vec<u16> = crate::osd::glyph::KEYBOARD.to_string().encode_utf16().collect();
        let mut rc = RECT { left: 0, top: 0, right: size, bottom: size };
        DrawTextW(
            dc,
            glyph.as_mut_ptr(),
            glyph.len() as i32,
            &mut rc,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE,
        );
        SelectObject(dc, old_font);
        DeleteObject(font);

        let fg: (u8, u8, u8) = if taskbar_light() { (0x1A, 0x1A, 0x1A) } else { (0xFF, 0xFF, 0xFF) };
        let opacity = if look == Look::Dimmed { 0.45 } else { 1.0 };
        let px = std::slice::from_raw_parts_mut(bits as *mut u32, (size * size) as usize);
        let dot_r = size as f32 * 0.2;
        let (cx, cy) = (size as f32 - dot_r - 0.5, size as f32 - dot_r - 0.5);
        for y in 0..size {
            for x in 0..size {
                let i = (y * size + x) as usize;
                let cov = (px[i] >> 16 & 0xFF) as f32 / 255.0;
                let mut a = cov * opacity;
                let (mut r, mut g, mut b) = (fg.0 as f32, fg.1 as f32, fg.2 as f32);
                if look == Look::LowBattery {
                    let d = ((x as f32 - cx).powi(2) + (y as f32 - cy).powi(2)).sqrt();
                    let dot = (dot_r + 0.5 - d).clamp(0.0, 1.0);
                    if dot > 0.0 {
                        // Red dot over the glyph's corner.
                        r = r * (1.0 - dot) + 0xE8 as f32 * dot;
                        g = g * (1.0 - dot) + 0x11 as f32 * dot;
                        b = b * (1.0 - dot) + 0x23 as f32 * dot;
                        a = a.max(dot);
                    }
                }
                // Icons use straight (not premultiplied) BGRA.
                let pa = (a * 255.0) as u32;
                let c = |v: f32| (v as u32).min(255);
                px[i] = pa << 24 | c(r) << 16 | c(g) << 8 | c(b);
            }
        }
        SelectObject(dc, old);
        DeleteDC(dc);

        let mask = CreateBitmap(size, size, 1, 1, null_mut());
        let info = ICONINFO { fIcon: 1, xHotspot: 0, yHotspot: 0, hbmMask: mask, hbmColor: bmp };
        let icon = CreateIconIndirect(&info);
        DeleteObject(mask);
        DeleteObject(bmp);
        icon
    }
}

fn data(hwnd: HWND) -> NOTIFYICONDATAW {
    let mut nid: NOTIFYICONDATAW = unsafe { zeroed() };
    nid.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
    nid.hWnd = hwnd;
    nid.uID = ID;
    nid
}

/// Adds the icon; false while the taskbar is not ready yet (early at sign-in).
pub fn add(hwnd: HWND, icon: HICON, tip: &str) -> bool {
    let mut nid = data(hwnd);
    nid.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP | NIF_SHOWTIP;
    nid.uCallbackMessage = WM_TRAY;
    nid.hIcon = icon;
    copy_wide(&mut nid.szTip, tip);
    unsafe {
        let ok = Shell_NotifyIconW(NIM_ADD, &nid) != 0;
        nid.Anonymous.uVersion = NOTIFYICON_VERSION_4;
        Shell_NotifyIconW(NIM_SETVERSION, &nid);
        ok
    }
}

pub fn update(hwnd: HWND, icon: HICON, tip: &str) {
    let mut nid = data(hwnd);
    nid.uFlags = NIF_ICON | NIF_TIP | NIF_SHOWTIP;
    nid.hIcon = icon;
    copy_wide(&mut nid.szTip, tip);
    unsafe { Shell_NotifyIconW(NIM_MODIFY, &nid) };
}

/// Shows a Windows notification attributed to the tray icon.
pub fn notify(hwnd: HWND, title: &str, text: &str, warning: bool) {
    let mut nid = data(hwnd);
    nid.uFlags = NIF_INFO;
    copy_wide(&mut nid.szInfoTitle, title);
    copy_wide(&mut nid.szInfo, text);
    nid.dwInfoFlags = if warning { NIIF_WARNING } else { NIIF_INFO };
    unsafe { Shell_NotifyIconW(NIM_MODIFY, &nid) };
}

pub fn remove(hwnd: HWND) {
    let nid = data(hwnd);
    unsafe { Shell_NotifyIconW(NIM_DELETE, &nid) };
}

pub fn destroy_icon(icon: HICON) {
    if !icon.is_null() {
        unsafe { DestroyIcon(icon) };
    }
}
