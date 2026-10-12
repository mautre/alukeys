//! User configuration (TOML) and its compiled form used by the engine.
//!
//! The file stores only the preset plus the user's changes, so improved presets in new
//! versions reach existing users automatically.

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};

use crate::action::{Action, Rule, Trigger};
use crate::keys::{Key, PhysMods};
use crate::presets::{self, Preset};

pub const CONFIG_VERSION: u32 = 1;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TopRow {
    /// Brightness, media and volume; hold fn for F1–F12 (the macOS default).
    #[default]
    Media,
    /// F1–F12; hold fn for media.
    Function,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Toggle {
    #[default]
    Auto,
    On,
    Off,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TopRowConfig {
    pub mode: TopRow,
    /// With Ctrl, Option, Cmd or Shift held, the top row always sends F1–F12
    /// (so Alt+F4, Ctrl+F5 and Shift+F10 work without fn).
    pub function_with_modifiers: bool,
    /// Per-key overrides of the media action, e.g. `F5 = "media_stop"`.
    pub keys: BTreeMap<String, String>,
}

impl Default for TopRowConfig {
    fn default() -> Self {
        TopRowConfig { mode: TopRow::Media, function_with_modifiers: true, keys: BTreeMap::new() }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RuleConfig {
    pub trigger: String,
    pub action: String,
    /// Extra modifiers allowed on top of the trigger's: "any", "shift" or "none".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extra: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BatteryConfig {
    /// Notify when the level drops to each of these percentages.
    pub warn_at: Vec<u8>,
}

impl Default for BatteryConfig {
    fn default() -> Self {
        BatteryConfig { warn_at: vec![20, 10, 5] }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UiConfig {
    /// "auto", "ru" or "en".
    pub language: String,
    /// Show on-screen indicators for fn lock and brightness.
    pub osd: bool,
    pub start_with_windows: bool,
    pub first_run_done: bool,
}

impl Default for UiConfig {
    fn default() -> Self {
        UiConfig { language: "auto".into(), osd: true, start_with_windows: true, first_run_done: false }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub version: u32,
    pub preset: Preset,
    pub top_row: TopRowConfig,
    /// Swap the key left of 1 with the key right of left Shift on ISO keyboards,
    /// matching where macOS puts them.
    pub iso_swap: Toggle,
    /// Physical key → action overrides of the base layer, e.g. `CapsLock = "LCtrl"`.
    pub remap: BTreeMap<String, String>,
    /// Built-in rule groups the user switched off.
    pub disabled: Vec<String>,
    /// The user's own shortcuts. A rule with the same trigger as a built-in replaces it.
    pub rules: Vec<RuleConfig>,
    /// Remap only while an Apple keyboard is connected (other keyboards stay untouched
    /// when it is away). Turn off to remap with any keyboard.
    pub only_with_apple: bool,
    pub battery: BatteryConfig,
    pub ui: UiConfig,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            version: CONFIG_VERSION,
            preset: Preset::Mac,
            top_row: TopRowConfig::default(),
            iso_swap: Toggle::Auto,
            remap: BTreeMap::new(),
            disabled: Vec::new(),
            rules: Vec::new(),
            only_with_apple: true,
            battery: BatteryConfig::default(),
            ui: UiConfig::default(),
        }
    }
}

impl Config {
    pub fn from_toml(s: &str) -> Result<Config, String> {
        toml::from_str(s).map_err(|e| e.to_string())
    }

    pub fn to_toml(&self) -> String {
        toml::to_string_pretty(self).expect("config serializes")
    }

    /// Builds the engine tables. `iso_keyboard` comes from the detected model and is
    /// used when `iso_swap` is `auto`. Invalid user entries are skipped and reported.
    pub fn compile(&self, iso_keyboard: bool) -> (Keymap, Vec<String>) {
        let mut errors = Vec::new();

        let mut remap: HashMap<Key, Action> = presets::remaps(self.preset).into_iter().collect();
        let swap = match self.iso_swap {
            Toggle::Auto => iso_keyboard,
            Toggle::On => true,
            Toggle::Off => false,
        };
        if swap {
            remap.insert(Key::GRAVE, Action::Key(Key::INTL_BACKSLASH));
            remap.insert(Key::INTL_BACKSLASH, Action::Key(Key::GRAVE));
        }
        for (k, a) in &self.remap {
            match (Key::parse(k), Action::parse(a)) {
                (Some(k), Ok(a)) => {
                    remap.insert(k, a);
                }
                (None, _) => errors.push(format!("remap: unknown key '{k}'")),
                (_, Err(e)) => errors.push(format!("remap {k}: {e}")),
            }
        }

        let mut rules: Vec<Rule> = presets::rules(self.preset)
            .into_iter()
            .filter(|r| !self.disabled.iter().any(|d| d == &r.group))
            .collect();
        for rc in &self.rules {
            let trigger = match Trigger::parse(&rc.trigger) {
                Ok(t) => t,
                Err(e) => {
                    errors.push(format!("rule: {e}"));
                    continue;
                }
            };
            let action = match Action::parse(&rc.action) {
                Ok(a) => a,
                Err(e) => {
                    errors.push(format!("rule {}: {e}", rc.trigger));
                    continue;
                }
            };
            let extra = match rc.extra.as_deref() {
                None => Rule::default_extra(&trigger),
                Some("any") => PhysMods::ALL.minus(trigger.mods),
                Some("shift") => PhysMods::SHIFT,
                Some("none") => PhysMods::NONE,
                Some(x) => {
                    errors.push(format!("rule {}: bad extra '{x}'", rc.trigger));
                    continue;
                }
            };
            rules.retain(|r| r.trigger != trigger);
            rules.push(Rule { group: "user".into(), trigger, action, extra });
        }

        let mut media: HashMap<Key, Action> = presets::media_row().into_iter().collect();
        for (k, a) in &self.top_row.keys {
            match (Key::parse(k), Action::parse(a)) {
                (Some(k), Ok(a)) if k.is_fkey() => {
                    media.insert(k, a);
                }
                _ => errors.push(format!("top_row: bad entry {k} = '{a}'")),
            }
        }

        let keymap = Keymap {
            remap,
            rules,
            media,
            functions_first: self.top_row.mode == TopRow::Function,
            function_with_modifiers: self.top_row.function_with_modifiers,
        };
        (keymap, errors)
    }
}

/// Compiled tables consumed by [`crate::engine::Engine`].
#[derive(Clone, Debug, Default)]
pub struct Keymap {
    pub remap: HashMap<Key, Action>,
    pub rules: Vec<Rule>,
    pub media: HashMap<Key, Action>,
    pub functions_first: bool,
    pub function_with_modifiers: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_roundtrip() {
        let c = Config::default();
        let s = c.to_toml();
        assert_eq!(Config::from_toml(&s).unwrap(), c);
        assert_eq!(Config::from_toml("").unwrap(), c);
    }

    #[test]
    fn user_overrides() {
        let c = Config::from_toml(
            r#"
            preset = "mac"
            disabled = ["ctrl-desktops"]
            [remap]
            CapsLock = "LCtrl"
            [[rules]]
            trigger = "Cmd+Space"
            action = "Win+S"
            [[rules]]
            trigger = "Cmd+Nope"
            action = "X"
            "#,
        )
        .unwrap();
        let (k, errors) = c.compile(true);
        assert_eq!(errors.len(), 1);
        assert_eq!(k.remap[&Key::CAPSLOCK], Action::Key(Key::LCTRL));
        assert_eq!(k.remap[&Key::GRAVE], Action::Key(Key::INTL_BACKSLASH));
        assert!(!k.rules.iter().any(|r| r.group == "ctrl-desktops"));
        let space: Vec<_> = k
            .rules
            .iter()
            .filter(|r| r.trigger.key == Key::SPACE && r.trigger.mods == PhysMods::CMD)
            .collect();
        assert_eq!(space.len(), 1);
        assert_eq!(space[0].action.to_string(), "Win+S");
    }
}
