//! Live session: a hidden message window receives WM_INPUT for all keyboards and for
//! Apple HID collections, while a WH_KEYBOARD_LL hook on the same thread logs (and for
//! the letter B, blocks) keystrokes. Timestamps come from QPC, so the log shows exactly
//! which arrives first and whether a blocked key still produces WM_INPUT.

use std::cell::RefCell;
use std::collections::HashMap;
use std::mem::{size_of, zeroed};
use std::ptr::null_mut;

use windows_sys::Win32::Foundation::{HANDLE, HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};
use windows_sys::Win32::UI::Input::{
    GetRawInputData, RAWINPUT, RAWINPUTDEVICE, RAWINPUTHEADER, RAWKEYBOARD, RID_INPUT, RIDEV_DEVNOTIFY,
    RIDEV_INPUTSINK, RIM_TYPEHID, RIM_TYPEKEYBOARD, RegisterRawInputDevices,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, HHOOK, HWND_MESSAGE,
    KBDLLHOOKSTRUCT, LLKHF_EXTENDED, LLKHF_INJECTED, MSG, PostQuitMessage, RegisterClassW, SetTimer,
    SetWindowsHookExW, UnhookWindowsHookEx, WH_KEYBOARD_LL, WM_INPUT, WM_INPUT_DEVICE_CHANGE, WM_KEYDOWN,
    WM_SYSKEYDOWN, WM_TIMER, WNDCLASSW,
};

use crate::enumerate::{AppleCollection, device_name};
use crate::out;
use crate::util::{hex, vid_pid_from_path, wide};

const SESSION_SECONDS: u32 = 240;
const VK_B: u32 = 0x42;
const VK_ESCAPE: u32 = 0x1B;

struct State {
    freq: i64,
    start: i64,
    esc_streak: u32,
    seconds: u32,
    names: HashMap<usize, String>,
}

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

fn now_ms() -> f64 {
    STATE.with(|s| {
        let s = s.borrow();
        let s = s.as_ref().unwrap();
        let mut t = 0i64;
        unsafe { QueryPerformanceCounter(&mut t) };
        (t - s.start) as f64 * 1000.0 / s.freq as f64
    })
}

fn short_device(h: HANDLE) -> String {
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        let s = s.as_mut().unwrap();
        s.names
            .entry(h as usize)
            .or_insert_with(|| {
                let name = device_name(h);
                match vid_pid_from_path(&name) {
                    Some((v, p)) => format!("{v:04X}:{p:04X}#{:X}", h as usize & 0xFFFF),
                    None => format!("dev#{:X}", h as usize & 0xFFFF),
                }
            })
            .clone()
    })
}

pub fn run(apple: &[AppleCollection]) {
    let mut freq = 0i64;
    let mut start = 0i64;
    unsafe {
        QueryPerformanceFrequency(&mut freq);
        QueryPerformanceCounter(&mut start);
    }
    STATE.with(|s| {
        *s.borrow_mut() = Some(State { freq, start, esc_streak: 0, seconds: 0, names: HashMap::new() })
    });

    unsafe {
        let hinst = GetModuleHandleW(null_mut());
        let class = wide("AluKeysDiag");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wnd_proc),
            hInstance: hinst,
            lpszClassName: class.as_ptr(),
            ..zeroed()
        };
        RegisterClassW(&wc);
        let hwnd = CreateWindowExW(
            0,
            class.as_ptr(),
            class.as_ptr(),
            0,
            0,
            0,
            0,
            0,
            HWND_MESSAGE,
            null_mut(),
            hinst,
            null_mut(),
        );

        let mut devices = vec![RAWINPUTDEVICE {
            usUsagePage: 0x01,
            usUsage: 0x06,
            dwFlags: RIDEV_INPUTSINK | RIDEV_DEVNOTIFY,
            hwndTarget: hwnd,
        }];
        for c in apple {
            // Keyboard/mouse collections are owned by Windows; register only the rest.
            if (c.usage_page, c.usage) == (0x01, 0x06) || (c.usage_page, c.usage) == (0x01, 0x02) {
                continue;
            }
            devices.push(RAWINPUTDEVICE {
                usUsagePage: c.usage_page,
                usUsage: c.usage,
                dwFlags: RIDEV_INPUTSINK,
                hwndTarget: hwnd,
            });
        }
        for d in &devices {
            let ok = RegisterRawInputDevices(d, 1, size_of::<RAWINPUTDEVICE>() as u32) != 0;
            out!(
                "RegisterRawInputDevices page=0x{:04X} usage=0x{:04X}: {}",
                d.usUsagePage,
                d.usUsage,
                if ok { "ok" } else { "FAILED" }
            );
        }

        let hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook_proc), hinst, 0);
        out!("Low-level keyboard hook: {}", if hook.is_null() { "FAILED" } else { "ok" });
        SetTimer(hwnd, 1, 1000, None);

        out!("");
        out!("Сейчас по очереди нажмите и отпустите на клавиатуре Apple:");
        out!("  1) fn            2) Eject (⏏)         3) fn+F1, затем F1 без fn");
        out!("  4) F7, F8, F10   5) Ctrl, Option, Cmd (слева и справа)");
        out!("  6) клавишу слева от 1 и клавишу справа от левого Shift");
        out!("  7) букву B (программа её заблокирует — это тест)");
        out!("  8) fn+Backspace, fn+стрелки");
        out!("  9) подержите fn+B две секунды");
        out!(" 10) выключите клавиатуру кнопкой питания сбоку, через 5 с включите и подождите,");
        out!("     пока она подключится (проверка: присылает ли она заряд батареи сама)");
        out!("Потом трижды подряд нажмите Esc (или подождите {SESSION_SECONDS} с).");
        out!("");

        let mut msg: MSG = zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            DispatchMessageW(&msg);
        }
        UnhookWindowsHookEx(hook as HHOOK);
    }
}

unsafe extern "system" fn hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let k = unsafe { &*(lparam as *const KBDLLHOOKSTRUCT) };
        let down = wparam as u32 == WM_KEYDOWN || wparam as u32 == WM_SYSKEYDOWN;
        let injected = k.flags & LLKHF_INJECTED != 0;
        let block = k.vkCode == VK_B && !injected;
        out!(
            "[{:10.3} ms] HOOK   {} vk=0x{:02X} sc=0x{:02X}{} flags=0x{:02X}{}{}",
            now_ms(),
            if down { "down" } else { "up  " },
            k.vkCode,
            k.scanCode,
            if k.flags & LLKHF_EXTENDED != 0 { " E0" } else { "   " },
            k.flags,
            if injected { " INJECTED" } else { "" },
            if block { "  ** BLOCKED by hook **" } else { "" }
        );
        if down {
            let quit = STATE.with(|s| {
                let mut s = s.borrow_mut();
                let s = s.as_mut().unwrap();
                s.esc_streak = if k.vkCode == VK_ESCAPE { s.esc_streak + 1 } else { 0 };
                s.esc_streak >= 3
            });
            if quit {
                unsafe { PostQuitMessage(0) };
            }
        }
        if block {
            return 1;
        }
    }
    unsafe { CallNextHookEx(null_mut(), code, wparam, lparam) }
}

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_INPUT => {
            unsafe { on_raw_input(lparam) };
            0
        }
        WM_INPUT_DEVICE_CHANGE => {
            let what = if wparam == 1 { "ARRIVAL" } else { "REMOVAL" };
            out!("[{:10.3} ms] DEVICE {what} {}", now_ms(), short_device(lparam as HANDLE));
            0
        }
        WM_TIMER => {
            let done = STATE.with(|s| {
                let mut s = s.borrow_mut();
                let s = s.as_mut().unwrap();
                s.seconds += 1;
                s.seconds >= SESSION_SECONDS
            });
            if done {
                out!("(time is up)");
                unsafe { PostQuitMessage(0) };
            }
            0
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

unsafe fn on_raw_input(lparam: LPARAM) {
    let t = now_ms();
    let hdr = size_of::<RAWINPUTHEADER>() as u32;
    let mut size = 0u32;
    unsafe { GetRawInputData(lparam as _, RID_INPUT, null_mut(), &mut size, hdr) };
    let mut buf = vec![0u64; (size as usize).div_ceil(8)];
    if unsafe { GetRawInputData(lparam as _, RID_INPUT, buf.as_mut_ptr().cast(), &mut size, hdr) } == u32::MAX
    {
        return;
    }
    let raw = unsafe { &*(buf.as_ptr() as *const RAWINPUT) };
    let dev = short_device(raw.header.hDevice);
    match raw.header.dwType {
        RIM_TYPEKEYBOARD => {
            let k: &RAWKEYBOARD = unsafe { &raw.data.keyboard };
            let up = k.Flags & 1 != 0;
            let e0 = k.Flags & 2 != 0;
            let e1 = k.Flags & 4 != 0;
            out!(
                "[{t:10.3} ms] RAWKB  {} vk=0x{:02X} sc=0x{:02X}{}{} dev={dev}",
                if up { "up  " } else { "down" },
                k.VKey,
                k.MakeCode,
                if e0 { " E0" } else { "" },
                if e1 { " E1" } else { "" },
            );
        }
        RIM_TYPEHID => {
            let hid = unsafe { &raw.data.hid };
            let total = (hid.dwSizeHid * hid.dwCount) as usize;
            let data = unsafe { std::slice::from_raw_parts(hid.bRawData.as_ptr(), total) };
            let mut line = format!("[{t:10.3} ms] RAWHID {} dev={dev}", hex(data));
            if data.len() >= 2 && data[0] == 0x47 {
                line.push_str(&format!("  -> BATTERY raw {} (~{}%)", data[1], data[1] as u32 * 100 / 255));
            }
            if data.len() >= 2 && data[0] == 0x11 {
                line.push_str(&format!(
                    "  -> fn={} eject={}",
                    if data[1] & 0x10 != 0 { "DOWN" } else { "up" },
                    if data[1] & 0x08 != 0 { "DOWN" } else { "up" }
                ));
            }
            out!("{line}");
        }
        _ => {}
    }
}
