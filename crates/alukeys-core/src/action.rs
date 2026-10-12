//! What a key or shortcut does, and the text syntax used in the config file and UI.
//!
//! Action syntax:
//! - `Home`: act as another key while held (auto-repeat and modifiers keep working).
//! - `Ctrl+Shift+Esc`: tap a chord. Use `tap:Home` to tap a bare key.
//! - `Shift+Home, Backspace`: tap several chords in order.
//! - `volume_up`, `play_pause`, `brightness_down`, `fn_lock`, `lock`, …: special actions.
//! - `switcher:Alt+Tab`: hold the chord's modifier while the trigger's modifier stays down.
//! - `vk:0x13`: tap a raw Windows virtual key.
//! - `text:—`: type these characters (any layout, any app).
//! - `none`: do nothing.
//!
//! Trigger syntax: Apple keycap modifiers plus a key, e.g. `Cmd+Shift+3`, `Fn+Left`, `Opt+Backspace`.

use std::fmt;

use crate::keys::{Key, OutMods, PhysMods, vk};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Chord {
    pub mods: OutMods,
    pub key: Key,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Action {
    None,
    Key(Key),
    Chord(Chord),
    Sequence(Vec<Chord>),
    Vk(u16),
    Brightness(i8),
    FnLockToggle,
    Lock,
    Switcher(Chord),
    Text(String),
}

const SPECIAL: &[(&str, Action)] = &[
    ("none", Action::None),
    ("fn_lock", Action::FnLockToggle),
    ("lock", Action::Lock),
    ("brightness_up", Action::Brightness(1)),
    ("brightness_down", Action::Brightness(-1)),
    ("volume_up", Action::Vk(vk::VOLUME_UP)),
    ("volume_down", Action::Vk(vk::VOLUME_DOWN)),
    ("mute", Action::Vk(vk::VOLUME_MUTE)),
    ("play_pause", Action::Vk(vk::MEDIA_PLAY_PAUSE)),
    ("next_track", Action::Vk(vk::MEDIA_NEXT)),
    ("prev_track", Action::Vk(vk::MEDIA_PREV)),
    ("media_stop", Action::Vk(vk::MEDIA_STOP)),
    ("browser_back", Action::Vk(vk::BROWSER_BACK)),
    ("browser_forward", Action::Vk(vk::BROWSER_FORWARD)),
    ("pause", Action::Vk(vk::PAUSE)),
];

fn parse_out_mod(s: &str) -> Option<OutMods> {
    Some(match s.to_ascii_lowercase().as_str() {
        "ctrl" | "control" => OutMods::CTRL,
        "alt" => OutMods::ALT,
        "shift" => OutMods::SHIFT,
        "win" | "meta" | "super" => OutMods::WIN,
        _ => return None,
    })
}

fn parse_phys_mod(s: &str) -> Option<PhysMods> {
    Some(match s.to_ascii_lowercase().as_str() {
        "cmd" | "command" => PhysMods::CMD,
        "opt" | "option" | "alt" => PhysMods::OPT,
        "ctrl" | "control" => PhysMods::CTRL,
        "shift" => PhysMods::SHIFT,
        "fn" => PhysMods::FN,
        _ => return None,
    })
}

impl Chord {
    pub fn parse(s: &str) -> Result<Chord, String> {
        let parts: Vec<&str> = s.split('+').map(str::trim).collect();
        // A trailing empty part means the key itself is '+', which we do not support.
        let (key, mods) = parts.split_last().ok_or_else(|| format!("empty chord '{s}'"))?;
        let key = Key::parse(key).ok_or_else(|| format!("unknown key '{key}' in '{s}'"))?;
        let mut m = OutMods::NONE;
        for p in mods {
            m = m | parse_out_mod(p).ok_or_else(|| format!("unknown modifier '{p}' in '{s}'"))?;
        }
        Ok(Chord { mods: m, key })
    }
}

impl fmt::Display for Chord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for m in self.mods.iter() {
            write!(f, "{m:?}+")?;
        }
        write!(f, "{}", self.key)
    }
}

impl Action {
    pub fn parse(s: &str) -> Result<Action, String> {
        let t = s.trim();
        if let Some((_, a)) = SPECIAL.iter().find(|(n, _)| n.eq_ignore_ascii_case(t)) {
            return Ok(a.clone());
        }
        if let Some(rest) = s.trim_start().strip_prefix("text:") {
            return if rest.is_empty() {
                Err("empty text".into())
            } else {
                Ok(Action::Text(rest.to_string()))
            };
        }
        if let Some(rest) = t.strip_prefix("vk:") {
            let v = u16::from_str_radix(rest.trim().trim_start_matches("0x"), 16)
                .map_err(|_| format!("bad vk '{t}'"))?;
            return Ok(Action::Vk(v));
        }
        if let Some(rest) = t.strip_prefix("switcher:") {
            return Ok(Action::Switcher(Chord::parse(rest)?));
        }
        if let Some(rest) = t.strip_prefix("tap:") {
            return Ok(Action::Chord(Chord::parse(rest)?));
        }
        if t.contains(',') {
            let seq = t.split(',').map(Chord::parse).collect::<Result<Vec<_>, _>>()?;
            return Ok(Action::Sequence(seq));
        }
        let c = Chord::parse(t)?;
        Ok(if c.mods == OutMods::NONE { Action::Key(c.key) } else { Action::Chord(c) })
    }
}

impl fmt::Display for Action {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some((n, _)) = SPECIAL.iter().find(|(_, a)| a == self) {
            return f.write_str(n);
        }
        match self {
            Action::Key(k) => write!(f, "{k}"),
            Action::Chord(c) if c.mods == OutMods::NONE => write!(f, "tap:{c}"),
            Action::Chord(c) => write!(f, "{c}"),
            Action::Sequence(v) => {
                let s: Vec<String> = v.iter().map(|c| c.to_string()).collect();
                f.write_str(&s.join(", "))
            }
            Action::Vk(v) => write!(f, "vk:0x{v:02X}"),
            Action::Switcher(c) => write!(f, "switcher:{c}"),
            Action::Text(t) => write!(f, "text:{t}"),
            Action::Brightness(d) => write!(f, "brightness:{d}"),
            _ => f.write_str("none"),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Trigger {
    pub mods: PhysMods,
    pub key: Key,
}

impl Trigger {
    pub fn parse(s: &str) -> Result<Trigger, String> {
        let parts: Vec<&str> = s.split('+').map(str::trim).collect();
        let (key, mods) = parts.split_last().ok_or_else(|| format!("empty trigger '{s}'"))?;
        let key = Key::parse(key)
            .or_else(|| match key.to_ascii_lowercase().as_str() {
                // Bare modifier names as the trigger key mean the left one.
                "cmd" | "command" => Some(Key::LWIN),
                "opt" | "option" => Some(Key::LALT),
                "ctrl" | "control" => Some(Key::LCTRL),
                "shift" => Some(Key::LSHIFT),
                _ => None,
            })
            .ok_or_else(|| format!("unknown key '{key}' in '{s}'"))?;
        let mut m = PhysMods::NONE;
        for p in mods {
            m = m | parse_phys_mod(p).ok_or_else(|| format!("unknown modifier '{p}' in '{s}'"))?;
        }
        Ok(Trigger { mods: m, key })
    }
}

impl fmt::Display for Trigger {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let m = format!("{:?}", self.mods);
        if m.is_empty() { write!(f, "{}", self.key) } else { write!(f, "{m}+{}", self.key) }
    }
}

/// A shortcut: when `trigger` is pressed, run `action`.
///
/// Physical modifiers beyond the trigger's own are allowed only if they are in `extra`;
/// they stay pressed for the output (so Shift extends a selection, for instance).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Rule {
    pub group: String,
    pub trigger: Trigger,
    pub action: Action,
    pub extra: PhysMods,
}

impl Rule {
    /// Default policy for extra modifiers: fn-layer rules let every other modifier
    /// through; other rules allow Shift unless Shift is already part of the trigger.
    pub fn default_extra(trigger: &Trigger) -> PhysMods {
        if trigger.mods == PhysMods::FN {
            PhysMods::ALL.minus(PhysMods::FN)
        } else if trigger.mods.contains(PhysMods::SHIFT) {
            PhysMods::NONE
        } else {
            PhysMods::SHIFT
        }
    }

    pub fn new(group: &str, trigger: &str, action: &str) -> Rule {
        let trigger = Trigger::parse(trigger).expect("built-in trigger");
        Rule {
            group: group.to_string(),
            extra: Rule::default_extra(&trigger),
            trigger,
            action: Action::parse(action).expect("built-in action"),
        }
    }

    pub fn matches(&self, key: Key, held: PhysMods) -> bool {
        self.trigger.key == key
            && held.contains(self.trigger.mods)
            && self.extra.contains(held.minus(self.trigger.mods))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_actions() {
        assert_eq!(Action::parse("Home").unwrap(), Action::Key(Key::HOME));
        assert_eq!(
            Action::parse("Ctrl+Shift+Esc").unwrap(),
            Action::Chord(Chord { mods: OutMods::CTRL | OutMods::SHIFT, key: Key::ESC })
        );
        assert_eq!(
            Action::parse("tap:LWin").unwrap(),
            Action::Chord(Chord { mods: OutMods::NONE, key: Key::LWIN })
        );
        assert_eq!(Action::parse("volume_up").unwrap(), Action::Vk(vk::VOLUME_UP));
        assert_eq!(
            Action::parse("Shift+Home, Backspace").unwrap(),
            Action::Sequence(vec![
                Chord { mods: OutMods::SHIFT, key: Key::HOME },
                Chord { mods: OutMods::NONE, key: Key::BACKSPACE }
            ])
        );
        assert!(Action::parse("Hyper+X").is_err());
        for s in ["Home", "Ctrl+Shift+Esc", "tap:LWin", "volume_up", "switcher:Alt+Tab", "vk:0x5F", "pause"] {
            assert_eq!(Action::parse(s).unwrap().to_string(), s);
        }
    }

    #[test]
    fn rule_matching() {
        let r = Rule::new("t", "Cmd+Left", "Home");
        assert!(r.matches(Key::LEFT, PhysMods::CMD));
        assert!(r.matches(Key::LEFT, PhysMods::CMD | PhysMods::SHIFT));
        assert!(!r.matches(Key::LEFT, PhysMods::CMD | PhysMods::OPT));
        assert!(!r.matches(Key::LEFT, PhysMods::NONE));
        let f = Rule::new("t", "Fn+Left", "Home");
        assert!(f.matches(Key::LEFT, PhysMods::FN | PhysMods::CTRL | PhysMods::SHIFT));
        let s = Rule::new("t", "Cmd+Shift+3", "PrintScreen");
        assert!(s.matches(Key::N3, PhysMods::CMD | PhysMods::SHIFT));
        assert!(!s.matches(Key::N3, PhysMods::CMD));
    }
}
