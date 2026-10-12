//! State shared between the UI thread and the engine thread, plus the private window
//! messages they exchange.

use std::collections::VecDeque;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;

use alukeys_core::model::Model;
use alukeys_core::{Key, Keymap, Out};
use windows_sys::Win32::UI::WindowsAndMessaging::WM_APP;

use crate::util::Hwnd;

// Engine thread → UI thread.
/// wParam: effect kind (`FX_*`), lParam: value.
pub const WM_ENGINE_EFFECT: u32 = WM_APP + 1;
/// An Apple keyboard appeared or disappeared; read `SHARED.device`.
pub const WM_DEVICE_CHANGED: u32 = WM_APP + 2;
/// lParam: battery percent from an input report.
pub const WM_BATTERY: u32 = WM_APP + 3;

/// Traced key events are waiting in `TRACE`.
pub const WM_TRACE: u32 = WM_APP + 5;

// Worker → UI thread.
/// lParam: brightness percent after a change, or -1 if no monitor supports DDC/CI.
pub const WM_BRIGHTNESS_DONE: u32 = WM_APP + 4;

// UI thread → engine thread.
/// wParam: command (`CMD_*`), lParam: argument.
pub const WM_ENGINE_CMD: u32 = WM_APP + 10;

// Tray icon callback.
pub const WM_TRAY: u32 = WM_APP + 20;

pub const FX_BRIGHTNESS: usize = 1;
pub const FX_FN_LOCK: usize = 2;
pub const FX_LOCK: usize = 3;

pub const CMD_SET_PAUSED: usize = 1;
pub const CMD_RELOAD_KEYMAP: usize = 2;
pub const CMD_SET_FUNCTIONS_FIRST: usize = 3;
pub const CMD_QUIT: usize = 4;
pub const CMD_RESET: usize = 5;

#[derive(Clone, Debug, Default)]
pub struct Device {
    pub connected: bool,
    pub model: Option<Model>,
    pub pid: u16,
    pub product: String,
    /// HID interface paths of the Apple collections (for battery feature reports).
    pub paths: Vec<String>,
}

pub struct Shared {
    pub ui: Hwnd,
    pub engine: Hwnd,
    pub device: Device,
    pub battery: Option<u8>,
    pub paused: bool,
    /// Keymap waiting to be picked up by the engine thread.
    pub pending_keymap: Option<Keymap>,
    pub only_with_apple: bool,
}

pub static SHARED: Mutex<Shared> = Mutex::new(Shared {
    ui: Hwnd(0),
    engine: Hwnd(0),
    device: Device { connected: false, model: None, pid: 0, product: String::new(), paths: Vec::new() },
    battery: None,
    paused: false,
    pending_keymap: None,
    only_with_apple: true,
});

pub fn shared() -> std::sync::MutexGuard<'static, Shared> {
    SHARED.lock().unwrap_or_else(|e| e.into_inner())
}

/// A physical key event and what the engine sent for it (settings key tester).
pub struct Trace {
    pub key: Key,
    pub down: bool,
    /// Apple modifiers held before this event (as the engine sees them).
    pub mods: alukeys_core::PhysMods,
    pub pass: bool,
    pub out: Vec<Out>,
}

/// Tracing is on while the settings window is open.
pub static TRACE_ON: AtomicBool = AtomicBool::new(false);
/// Write every key event to the log (tray menu, for troubleshooting).
pub static LOG_KEYS: AtomicBool = AtomicBool::new(false);
pub static TRACE: Mutex<VecDeque<Trace>> = Mutex::new(VecDeque::new());
