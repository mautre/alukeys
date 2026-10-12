//! Sends the engine's output to Windows with SendInput.
//!
//! Every injected event carries `TAG` in dwExtraInfo, so our own hook recognises and
//! ignores it.

use std::mem::size_of;

use alukeys_core::Out;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, KEYEVENTF_SCANCODE,
    KEYEVENTF_UNICODE, MAPVK_VK_TO_VSC_EX, MapVirtualKeyW, SendInput,
};

/// "ALUK".
pub const TAG: usize = 0x414C_554B;

fn key_input(vk: u16, scan: u16, flags: u32) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT { wVk: vk, wScan: scan, dwFlags: flags, time: 0, dwExtraInfo: TAG },
        },
    }
}

pub fn send(out: &[Out]) {
    if out.is_empty() {
        return;
    }
    let inputs: Vec<INPUT> = out
        .iter()
        .map(|o| match *o {
            Out::Key { key, down } => {
                let mut flags = KEYEVENTF_SCANCODE;
                if key.is_extended() {
                    flags |= KEYEVENTF_EXTENDEDKEY;
                }
                if !down {
                    flags |= KEYEVENTF_KEYUP;
                }
                key_input(0, key.scan_code(), flags)
            }
            Out::Vk { vk, down } => {
                // MAPVK_VK_TO_VSC_EX reports E0-prefixed keys (media keys) as 0xE0xx.
                let sc = unsafe { MapVirtualKeyW(vk as u32, MAPVK_VK_TO_VSC_EX) };
                let mut flags = if sc & 0xFF00 == 0xE000 { KEYEVENTF_EXTENDEDKEY } else { 0 };
                if !down {
                    flags |= KEYEVENTF_KEYUP;
                }
                key_input(vk, (sc & 0xFF) as u16, flags)
            }
            Out::Unicode { unit, down } => {
                let flags = KEYEVENTF_UNICODE | if down { 0 } else { KEYEVENTF_KEYUP };
                key_input(0, unit, flags)
            }
        })
        .collect();
    unsafe { SendInput(inputs.len() as u32, inputs.as_ptr(), size_of::<INPUT>() as i32) };
}
