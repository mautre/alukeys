//! JSON the settings page renders from: config, device, compiled key tables and the
//! characters each key types in the installed keyboard layouts.

use alukeys_core::config::TopRowConfig;
use alukeys_core::keys::NAMES;
use alukeys_core::model::Variant;
use alukeys_core::presets::{self, GROUPS};
use alukeys_core::{Config, Keymap, PhysMods};
use serde_json::{Map, Value, json};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyboardLayoutList, HKL, MAPVK_VSC_TO_VK_EX, MapVirtualKeyExW, ToUnicodeEx,
};

use crate::autostart::Mode;
use crate::shared::shared;
use crate::util::tr;

fn mods_list(m: PhysMods) -> Vec<&'static str> {
    [
        (PhysMods::FN, "Fn"),
        (PhysMods::CTRL, "Ctrl"),
        (PhysMods::OPT, "Opt"),
        (PhysMods::CMD, "Cmd"),
        (PhysMods::SHIFT, "Shift"),
    ]
    .into_iter()
    .filter(|(b, _)| m.contains(*b))
    .map(|(_, n)| n)
    .collect()
}

fn tables(k: &Keymap) -> Value {
    let remap: Map<String, Value> =
        k.remap.iter().map(|(key, a)| (key.name(), Value::String(a.to_string()))).collect();
    let media: Map<String, Value> =
        k.media.iter().map(|(key, a)| (key.name(), Value::String(a.to_string()))).collect();
    let rules: Vec<Value> = k
        .rules
        .iter()
        .map(|r| {
            json!({
                "mods": mods_list(r.trigger.mods),
                "key": r.trigger.key.name(),
                "action": r.action.to_string(),
                "group": r.group,
            })
        })
        .collect();
    json!({ "remap": remap, "media": media, "rules": rules })
}

/// What each key types in every installed layout: `[{lang, keys: {Q: ["й", "Й"]}}]`.
fn layouts() -> Value {
    let mut hkls: [HKL; 16] = [std::ptr::null_mut(); 16];
    let n = unsafe { GetKeyboardLayoutList(hkls.len() as i32, hkls.as_mut_ptr()) }.max(0) as usize;
    let mut out = Vec::new();
    for &hkl in &hkls[..n] {
        let lang = (hkl as usize & 0xFFFF) as u16;
        let tag = match lang & 0x3FF {
            0x09 => "en".to_string(),
            0x19 => "ru".to_string(),
            0x22 => "uk".to_string(),
            0x23 => "be".to_string(),
            0x3F => "kk".to_string(),
            0x07 => "de".to_string(),
            0x0C => "fr".to_string(),
            other => format!("{other:02x}"),
        };
        let mut keys = Map::new();
        for &(name, key) in NAMES {
            if key.is_extended() || key.is_virtual() || key.is_modifier() {
                continue;
            }
            let sc = key.scan_code() as u32;
            let vk = unsafe { MapVirtualKeyExW(sc, MAPVK_VSC_TO_VK_EX, hkl) };
            if vk == 0 {
                continue;
            }
            let mut pair = Vec::new();
            for shift in [false, true] {
                let mut state = [0u8; 256];
                if shift {
                    state[0x10] = 0x80;
                }
                let mut buf = [0u16; 8];
                // Flag 4: do not disturb the keyboard state (dead keys stay intact).
                let r = unsafe { ToUnicodeEx(vk, sc, state.as_ptr(), buf.as_mut_ptr(), 8, 4, hkl) };
                let s = if r > 0 { String::from_utf16_lossy(&buf[..r as usize]) } else { String::new() };
                pair.push(Value::String(s.chars().filter(|c| !c.is_control()).collect()));
            }
            if pair.iter().any(|p| !p.as_str().unwrap_or("").is_empty()) {
                keys.insert(name.to_string(), Value::Array(pair));
            }
        }
        if !out.iter().any(|v: &Value| v["lang"] == tag) {
            out.push(json!({ "lang": tag, "keys": keys }));
        }
    }
    Value::Array(out)
}

pub struct Context<'a> {
    pub config: &'a Config,
    pub iso: bool,
    pub autostart: Mode,
    pub elevated: bool,
    pub errors: Vec<String>,
}

pub fn state(c: &Context) -> String {
    let (effective, _) = c.config.compile(c.iso);
    // Defaults for this preset, to show which keys the user changed.
    let base_cfg = Config {
        preset: c.config.preset,
        iso_swap: c.config.iso_swap,
        top_row: TopRowConfig { keys: Default::default(), ..c.config.top_row.clone() },
        ..Config::default()
    };
    let (base, _) = base_cfg.compile(c.iso);
    let preset_groups: std::collections::BTreeSet<String> =
        presets::rules(c.config.preset).into_iter().map(|r| r.group).collect();
    let groups: Vec<Value> = GROUPS
        .iter()
        .filter(|g| preset_groups.contains(g.id))
        .map(|g| {
            json!({
                "id": g.id,
                "title": tr(g.title_ru, g.title_en),
                "enabled": !c.config.disabled.iter().any(|d| d == g.id),
            })
        })
        .collect();

    let sh = shared();
    let variant = sh.device.model.map(|m| match m.variant {
        Variant::Ansi => "ansi",
        Variant::Iso => "iso",
        Variant::Jis => "jis",
    });
    let name = sh.device.model.map(|m| m.name.to_string()).unwrap_or_else(|| sh.device.product.clone());
    let state = json!({
        "type": "state",
        "version": env!("CARGO_PKG_VERSION"),
        "lang": tr("ru", "en"),
        "config": c.config,
        "device": {
            "connected": sh.device.connected,
            "name": name,
            "pid": format!("{:04X}", sh.device.pid),
            "variant": variant,
            "battery": sh.battery,
        },
        "paused": sh.paused,
        "autostart": match c.autostart { Mode::Off => "off", Mode::Normal => "normal", Mode::Elevated => "elevated" },
        "elevated": c.elevated,
        "groups": groups,
        "effective": tables(&effective),
        "defaults": tables(&base),
        "layouts": layouts(),
        "errors": c.errors,
        "configPath": crate::settings::config_path().display().to_string(),
    });
    drop(sh);
    state.to_string()
}

/// One line of the key log: `down LWin [Fn] -> LCtrl+` (or `pass`).
pub fn trace_line(t: &crate::shared::Trace) -> String {
    let out: Vec<String> = t
        .out
        .iter()
        .map(|o| match o {
            alukeys_core::Out::Key { key, down } => format!("{key}{}", if *down { "+" } else { "-" }),
            alukeys_core::Out::Vk { vk, down } => format!("vk{vk:02X}{}", if *down { "+" } else { "-" }),
            alukeys_core::Out::Unicode { unit, down } => {
                format!("U+{unit:04X}{}", if *down { "+" } else { "-" })
            }
        })
        .collect();
    format!(
        "key {} {} held=[{}] -> {}",
        if t.down { "down" } else { "up  " },
        t.key.name(),
        mods_list(t.mods).join(" "),
        if t.pass {
            "pass".to_string()
        } else if out.is_empty() {
            "swallow".to_string()
        } else {
            out.join(" ")
        }
    )
}

/// One traced key event for the live tester.
pub fn trace_json(events: &[crate::shared::Trace]) -> String {
    let list: Vec<Value> = events
        .iter()
        .map(|t| {
            let out: Vec<Value> = t
                .out
                .iter()
                .map(|o| match o {
                    alukeys_core::Out::Key { key, down } => {
                        json!({ "key": key.name(), "down": down })
                    }
                    alukeys_core::Out::Vk { vk, down } => json!({ "vk": vk, "down": down }),
                    alukeys_core::Out::Unicode { unit, down } => json!({ "unit": unit, "down": down }),
                })
                .collect();
            json!({ "key": t.key.name(), "down": t.down, "mods": mods_list(t.mods), "pass": t.pass, "out": out })
        })
        .collect();
    json!({ "type": "trace", "events": list }).to_string()
}
