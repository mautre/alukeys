//! Prints a settings-page state with real compiled tables, for previewing
//! `crates/alukeys/ui/settings.html` in a browser without Windows.

use alukeys_core::config::TopRowConfig;
use alukeys_core::keys::NAMES;
use alukeys_core::presets::{self, GROUPS};
use alukeys_core::{Config, Keymap, PhysMods};
use serde_json::{Map, Value, json};

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
        k.remap.iter().map(|(key, a)| (key.name(), a.to_string().into())).collect();
    let media: Map<String, Value> =
        k.media.iter().map(|(key, a)| (key.name(), a.to_string().into())).collect();
    let rules: Vec<Value> = k
        .rules
        .iter()
        .map(|r| json!({"mods": mods_list(r.trigger.mods), "key": r.trigger.key.name(), "action": r.action.to_string(), "group": r.group}))
        .collect();
    json!({"remap": remap, "media": media, "rules": rules})
}

fn main() {
    let config = Config::default();
    let (eff, _) = config.compile(true);
    let base_cfg = Config {
        top_row: TopRowConfig { keys: Default::default(), ..config.top_row.clone() },
        ..Config::default()
    };
    let (base, _) = base_cfg.compile(true);
    let groups: Vec<Value> =
        GROUPS.iter().map(|g| json!({"id": g.id, "title": g.title_ru, "enabled": true})).collect();
    let _ = presets::rules(config.preset);

    let ru: &[(&str, &str)] = &[
        ("Q", "й"),
        ("W", "ц"),
        ("E", "у"),
        ("R", "к"),
        ("T", "е"),
        ("Y", "н"),
        ("U", "г"),
        ("I", "ш"),
        ("O", "щ"),
        ("P", "з"),
        ("LBracket", "х"),
        ("RBracket", "ъ"),
        ("A", "ф"),
        ("S", "ы"),
        ("D", "в"),
        ("F", "а"),
        ("G", "п"),
        ("H", "р"),
        ("J", "о"),
        ("K", "л"),
        ("L", "д"),
        ("Semicolon", "ж"),
        ("Quote", "э"),
        ("Z", "я"),
        ("X", "ч"),
        ("C", "с"),
        ("V", "м"),
        ("B", "и"),
        ("N", "т"),
        ("M", "ь"),
        ("Comma", "б"),
        ("Period", "ю"),
        ("Slash", "."),
        ("Grave", "ё"),
        ("Backslash", "\\"),
        ("IntlBackslash", "\\"),
    ];
    let en_chars: &[(&str, &str)] = &[
        ("Grave", "`"),
        ("Minus", "-"),
        ("Equal", "="),
        ("LBracket", "["),
        ("RBracket", "]"),
        ("Semicolon", ";"),
        ("Quote", "'"),
        ("Backslash", "\\"),
        ("IntlBackslash", "\\"),
        ("Comma", ","),
        ("Period", "."),
        ("Slash", "/"),
    ];
    let mut en = Map::new();
    let mut ruk = Map::new();
    for &(name, _) in NAMES {
        let ch = if name.len() == 1 {
            Some(name.to_lowercase())
        } else {
            en_chars.iter().find(|(n, _)| *n == name).map(|(_, c)| c.to_string())
        };
        if let Some(c) = ch {
            en.insert(name.into(), json!([c, c.to_uppercase()]));
            let r = ru.iter().find(|(n, _)| *n == name).map(|(_, c)| c.to_string()).unwrap_or(c.clone());
            ruk.insert(name.into(), json!([r, r.to_uppercase()]));
        }
    }
    let state = json!({
        "type": "state", "version": env!("CARGO_PKG_VERSION"), "lang": "ru", "config": config,
        "device": {"connected": true, "name": "Apple Wireless Keyboard (A1314, 2011)", "pid": "0256", "variant": "iso", "battery": 64},
        "paused": false, "autostart": "normal", "elevated": false, "groups": groups,
        "effective": tables(&eff), "defaults": tables(&base),
        "layouts": [{"lang": "en", "keys": en}, {"lang": "ru", "keys": ruk}],
        "errors": [], "configPath": "C:\\Users\\me\\AppData\\Roaming\\AluKeys\\config.toml",
    });
    println!("{state}");
}
