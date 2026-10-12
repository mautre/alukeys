//! The engine thread: owns the remapping engine, the low-level keyboard hook and the raw
//! input window. Nothing slow runs here, so the hook always answers within microseconds.
//!
//! fn and Eject reach this thread as WM_INPUT; keystrokes come through the hook. Both
//! are queued to this one thread, so they are handled in the order the keyboard sent them.

use std::cell::RefCell;
use std::collections::HashMap;
use std::mem::{size_of, zeroed};
use std::ptr::null_mut;
use std::sync::mpsc;
use std::time::Instant;

use alukeys_core::model::Variant;
use alukeys_core::{Effect, Engine, Key, Keymap, Response};
use windows_sys::Win32::Foundation::{HANDLE, HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::{GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_HIGHEST};
use windows_sys::Win32::UI::Input::{
    GetRawInputData, RAWINPUT, RAWINPUTHEADER, RID_INPUT, RIM_TYPEHID, RIM_TYPEKEYBOARD,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW, HC_ACTION,
    HHOOK, HWND_MESSAGE, KBDLLHOOKSTRUCT, KillTimer, LLKHF_EXTENDED, LLKHF_INJECTED, MSG, PostMessageW,
    PostQuitMessage, RegisterClassW, SPI_GETKEYBOARDDELAY, SPI_GETKEYBOARDSPEED, SetTimer, SetWindowsHookExW,
    SystemParametersInfoW, UnhookWindowsHookEx, WH_KEYBOARD_LL, WM_INPUT, WM_INPUT_DEVICE_CHANGE, WM_KEYDOWN,
    WM_SYSKEYDOWN, WM_TIMER, WNDCLASSW,
};

use crate::devices::{self, Collection};
use crate::shared::*;
use crate::util::{Hwnd, wide};
use crate::{inject, log};

const TIMER_REPEAT: usize = 1;
const TIMER_RESCAN: usize = 2;
const TIMER_WATCHDOG: usize = 3;
const VK_PAUSE: u32 = 0x13;

struct State {
    hwnd: HWND,
    engine: Engine,
    hook: HHOOK,
    active: bool,
    fn_down: bool,
    eject_down: bool,
    repeat: Option<Key>,
    repeat_started: bool,
    started: Instant,
    last_hook_ms: u128,
    raw_since_hook: u32,
    collections: Vec<Collection>,
    /// Per raw-input handle: is it Apple, and its preparsed data (battery decoding).
    apple: HashMap<usize, Option<Vec<u8>>>,
}

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

/// Runs `f` with the thread state unless it is already borrowed (re-entrancy).
fn with<R>(f: impl FnOnce(&mut State) -> R) -> Option<R> {
    STATE.with(|s| s.try_borrow_mut().ok().and_then(|mut s| s.as_mut().map(f)))
}

/// Starts the engine thread. Join it on exit so held keys are released first.
pub fn spawn(keymap: Keymap) -> std::thread::JoinHandle<()> {
    let (tx, rx) = mpsc::channel();
    let handle = std::thread::Builder::new()
        .name("engine".into())
        .spawn(move || run(keymap, tx))
        .expect("engine thread");
    let _ = rx.recv();
    handle
}

fn run(keymap: Keymap, ready: mpsc::Sender<Hwnd>) {
    unsafe {
        SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_HIGHEST);
        let hinst = GetModuleHandleW(null_mut());
        let class = wide("AluKeysEngine");
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

        let (device, collections) = devices::scan();
        devices::register(hwnd, &collections);
        let only_with_apple = {
            let mut sh = shared();
            sh.engine = Hwnd(hwnd as usize);
            sh.device = device.clone();
            sh.only_with_apple
        };
        log!("engine: device {:?} pid={:04X} connected={}", device.product, device.pid, device.connected);

        let hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook_proc), hinst, 0);
        log!("engine: hook installed: {}", !hook.is_null());
        SetTimer(hwnd, TIMER_WATCHDOG, 2000, None);

        STATE.with(|s| {
            *s.borrow_mut() = Some(State {
                hwnd,
                engine: Engine::new(keymap),
                hook,
                active: device.connected || !only_with_apple,
                fn_down: false,
                eject_down: false,
                repeat: None,
                repeat_started: false,
                started: Instant::now(),
                last_hook_ms: 0,
                raw_since_hook: 0,
                collections,
                apple: HashMap::new(),
            })
        });
        let _ = ready.send(Hwnd(hwnd as usize));
        notify_ui(WM_DEVICE_CHANGED, 0, 0);

        let mut msg: MSG = zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            DispatchMessageW(&msg);
        }
        with(|s| UnhookWindowsHookEx(s.hook));
    }
}

fn notify_ui(msg: u32, w: usize, l: isize) {
    let ui = shared().ui;
    if ui.0 != 0 {
        unsafe { PostMessageW(ui.get(), msg, w, l) };
    }
}

/// Records an event for the settings key tester; wakes the UI on the first one queued.
fn trace(key: Key, down: bool, r: &Response) {
    use std::sync::atomic::Ordering;
    if !TRACE_ON.load(Ordering::Relaxed) {
        return;
    }
    let first = {
        let Ok(mut q) = TRACE.lock() else { return };
        if q.len() > 256 {
            return;
        }
        q.push_back(Trace { key, down, pass: r.pass, out: r.out.clone() });
        q.len() == 1
    };
    if first {
        notify_ui(WM_TRACE, 0, 0);
    }
}

impl State {
    /// Runs one physical event through the engine and performs the result.
    fn feed(&mut self, key: Key, down: bool) -> bool {
        let r = if self.active {
            self.engine.handle(key, down)
        } else {
            Response { pass: true, ..Default::default() }
        };
        trace(key, down, &r);
        let pass = r.pass;
        self.apply(r);
        pass
    }

    fn apply(&mut self, r: Response) {
        inject::send(&r.out);
        for e in r.effects {
            match e {
                Effect::Brightness(d) => notify_ui(WM_ENGINE_EFFECT, FX_BRIGHTNESS, d as isize),
                Effect::FnLock(on) => notify_ui(WM_ENGINE_EFFECT, FX_FN_LOCK, on as isize),
                Effect::Lock => notify_ui(WM_ENGINE_EFFECT, FX_LOCK, 0),
                Effect::StartRepeat(k) => {
                    self.repeat = Some(k);
                    self.repeat_started = false;
                    let (delay, _) = repeat_timing();
                    unsafe { SetTimer(self.hwnd, TIMER_REPEAT, delay, None) };
                }
                Effect::StopRepeat(k) => {
                    if self.repeat == Some(k) {
                        self.repeat = None;
                        unsafe { KillTimer(self.hwnd, TIMER_REPEAT) };
                    }
                }
            }
        }
    }

    fn set_active(&mut self, active: bool) {
        if self.active && !active {
            let r = self.engine.reset();
            self.apply(r);
            self.fn_down = false;
            self.eject_down = false;
        }
        self.active = active;
    }

    fn now_ms(&self) -> u128 {
        self.started.elapsed().as_millis()
    }
}

/// System keyboard repeat delay and interval in milliseconds.
fn repeat_timing() -> (u32, u32) {
    let mut delay = 1u32;
    let mut speed = 31u32;
    unsafe {
        SystemParametersInfoW(SPI_GETKEYBOARDDELAY, 0, (&mut delay as *mut u32).cast(), 0);
        SystemParametersInfoW(SPI_GETKEYBOARDSPEED, 0, (&mut speed as *mut u32).cast(), 0);
    }
    // delay 0..3 → 250..1000 ms; speed 0..31 → ~2.5..30 repeats per second.
    let delay_ms = 250 * (delay.min(3) + 1);
    let rate = 2.5 + speed.min(31) as f32 * (27.5 / 31.0);
    (delay_ms, (1000.0 / rate) as u32)
}

unsafe extern "system" fn hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let k = unsafe { &*(lparam as *const KBDLLHOOKSTRUCT) };
        let ours = k.dwExtraInfo == inject::TAG;
        let foreign = k.flags & LLKHF_INJECTED != 0;
        if !ours && !foreign && k.scanCode != 0 && k.vkCode != VK_PAUSE {
            let down = wparam as u32 == WM_KEYDOWN || wparam as u32 == WM_SYSKEYDOWN;
            let key = Key::from_scan(k.scanCode as u16, k.flags & LLKHF_EXTENDED != 0);
            let swallow = with(|s| {
                s.last_hook_ms = s.now_ms();
                s.raw_since_hook = 0;
                !s.feed(key, down)
            });
            if swallow == Some(true) {
                return 1;
            }
        }
    }
    unsafe { CallNextHookEx(null_mut(), code, wparam, lparam) }
}

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_INPUT => {
            unsafe { on_input(lparam) };
            0
        }
        WM_INPUT_DEVICE_CHANGE => {
            // Devices arrive as bursts of collections; rescan once things settle.
            unsafe { SetTimer(hwnd, TIMER_RESCAN, 400, None) };
            0
        }
        WM_TIMER => {
            match wparam {
                TIMER_REPEAT => with(|s| {
                    let Some(k) = s.repeat else { return };
                    if !s.repeat_started {
                        s.repeat_started = true;
                        let (_, interval) = repeat_timing();
                        unsafe { SetTimer(s.hwnd, TIMER_REPEAT, interval, None) };
                    }
                    let r = s.engine.handle(k, true);
                    s.apply(r);
                }),
                TIMER_RESCAN => {
                    unsafe { KillTimer(hwnd, TIMER_RESCAN) };
                    with(rescan)
                }
                TIMER_WATCHDOG => with(watchdog),
                _ => None,
            };
            0
        }
        WM_ENGINE_CMD => {
            with(|s| command(s, wparam, lparam));
            0
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

fn command(s: &mut State, cmd: usize, arg: isize) {
    match cmd {
        CMD_SET_PAUSED => {
            let r = s.engine.set_paused(arg != 0);
            s.apply(r);
            shared().paused = arg != 0;
        }
        CMD_RELOAD_KEYMAP => {
            let (map, only_with_apple) = {
                let mut sh = shared();
                (sh.pending_keymap.take(), sh.only_with_apple)
            };
            if let Some(map) = map {
                let r = s.engine.set_keymap(map);
                s.apply(r);
            }
            let connected = shared().device.connected;
            s.set_active(connected || !only_with_apple);
        }
        CMD_SET_FUNCTIONS_FIRST => s.engine.set_functions_first(arg != 0),
        CMD_RESET => {
            let r = s.engine.reset();
            s.apply(r);
            s.fn_down = false;
            s.eject_down = false;
        }
        CMD_QUIT => {
            let r = s.engine.reset();
            s.apply(r);
            unsafe {
                UnhookWindowsHookEx(s.hook);
                s.hook = null_mut();
                DestroyWindow(s.hwnd);
                PostQuitMessage(0);
            }
        }
        _ => {}
    }
}

fn rescan(s: &mut State) {
    let (device, collections) = devices::scan();
    let new: Vec<Collection> = collections.iter().filter(|c| !s.collections.contains(c)).copied().collect();
    if !new.is_empty() {
        devices::register(s.hwnd, &new);
        s.collections.extend(new);
    }
    s.apple.clear();
    let (only_with_apple, was) = {
        let mut sh = shared();
        let was = sh.device.connected;
        sh.device = device.clone();
        (sh.only_with_apple, was)
    };
    if was != device.connected {
        log!("engine: keyboard {}", if device.connected { "connected" } else { "disconnected" });
    }
    s.set_active(device.connected || !only_with_apple);
    notify_ui(WM_DEVICE_CHANGED, 0, 0);
}

/// Windows silently drops a low-level hook that ever answers too slowly. If keystrokes
/// keep arriving as raw input while the hook stays quiet, install it again.
fn watchdog(s: &mut State) {
    if s.raw_since_hook >= 4 && s.now_ms() - s.last_hook_ms > 1500 {
        log!("engine: hook stopped receiving events; reinstalling");
        unsafe {
            UnhookWindowsHookEx(s.hook);
            s.hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook_proc), GetModuleHandleW(null_mut()), 0);
        }
        let r = s.engine.reset();
        s.apply(r);
        s.raw_since_hook = 0;
        s.last_hook_ms = s.now_ms();
    }
}

unsafe fn on_input(lparam: LPARAM) {
    let hdr = size_of::<RAWINPUTHEADER>() as u32;
    let mut size = 0u32;
    unsafe { GetRawInputData(lparam as _, RID_INPUT, null_mut(), &mut size, hdr) };
    let mut buf = vec![0u64; (size as usize).div_ceil(8)];
    if unsafe { GetRawInputData(lparam as _, RID_INPUT, buf.as_mut_ptr().cast(), &mut size, hdr) } == u32::MAX
    {
        return;
    }
    let raw = unsafe { &*(buf.as_ptr() as *const RAWINPUT) };
    let device = raw.header.hDevice;
    match raw.header.dwType {
        RIM_TYPEKEYBOARD if !device.is_null() => {
            with(|s| s.raw_since_hook += 1);
        }
        RIM_TYPEHID => {
            let hid = unsafe { &raw.data.hid };
            let total = (hid.dwSizeHid * hid.dwCount) as usize;
            let data = unsafe { std::slice::from_raw_parts(hid.bRawData.as_ptr(), total) };
            let report = &data[..(hid.dwSizeHid as usize).min(data.len())];
            with(|s| on_hid(s, device, report));
        }
        _ => {}
    }
}

fn on_hid(s: &mut State, device: HANDLE, report: &[u8]) {
    let entry = s.apple.entry(device as usize).or_insert_with(|| {
        let apple = devices::vid_pid(&devices::device_name(device))
            .is_some_and(|(v, _)| v == alukeys_core::model::APPLE_VID);
        apple.then(|| devices::preparsed(device).unwrap_or_default())
    });
    let Some(pp) = entry else { return };
    if !pp.is_empty()
        && let Some(level) = devices::battery_from_input(pp, report)
    {
        notify_ui(WM_BATTERY, 0, level as isize);
    }
    let Some((fn_down, eject_down)) = devices::parse_fn_report(report) else {
        return;
    };
    if fn_down != s.fn_down {
        s.fn_down = fn_down;
        s.feed(Key::FN, fn_down);
    }
    if eject_down != s.eject_down {
        s.eject_down = eject_down;
        s.feed(Key::EJECT, eject_down);
    }
}

/// Whether the detected keyboard has the ISO layout (decides the auto key swap).
pub fn is_iso() -> bool {
    shared().device.model.is_some_and(|m| m.variant == Variant::Iso)
}
