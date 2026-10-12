//! Key identities.
//!
//! A physical or output key is identified by its Windows "Set 1" scan code, with
//! `0xE000` added for E0-prefixed (extended) keys. This is layout independent: injecting
//! scan code 0x2E produces Ctrl+C with both the English and the Russian layout.
//! Keys that Windows does not see as keyboard keys (fn, Eject) live in `0xF000..`.

use std::fmt;

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Key(pub u16);

impl Key {
    pub const fn sc(sc: u16) -> Key {
        Key(sc)
    }
    pub const fn ext(sc: u16) -> Key {
        Key(0xE000 | sc)
    }
    pub fn from_scan(sc: u16, extended: bool) -> Key {
        if extended { Key::ext(sc & 0xFF) } else { Key(sc & 0xFF) }
    }
    pub fn scan_code(self) -> u16 {
        self.0 & 0xFF
    }
    pub fn is_extended(self) -> bool {
        self.0 & 0xFF00 == 0xE000
    }
    /// Keys produced by the Apple vendor HID report rather than the keyboard collection.
    pub fn is_virtual(self) -> bool {
        self.0 & 0xF000 == 0xF000
    }

    pub const ESC: Key = Key(0x01);
    pub const N1: Key = Key(0x02);
    pub const N2: Key = Key(0x03);
    pub const N3: Key = Key(0x04);
    pub const N4: Key = Key(0x05);
    pub const N5: Key = Key(0x06);
    pub const N6: Key = Key(0x07);
    pub const N7: Key = Key(0x08);
    pub const N8: Key = Key(0x09);
    pub const N9: Key = Key(0x0A);
    pub const N0: Key = Key(0x0B);
    pub const MINUS: Key = Key(0x0C);
    pub const EQUAL: Key = Key(0x0D);
    pub const BACKSPACE: Key = Key(0x0E);
    pub const TAB: Key = Key(0x0F);
    pub const Q: Key = Key(0x10);
    pub const W: Key = Key(0x11);
    pub const E: Key = Key(0x12);
    pub const R: Key = Key(0x13);
    pub const T: Key = Key(0x14);
    pub const Y: Key = Key(0x15);
    pub const U: Key = Key(0x16);
    pub const I: Key = Key(0x17);
    pub const O: Key = Key(0x18);
    pub const P: Key = Key(0x19);
    pub const LBRACKET: Key = Key(0x1A);
    pub const RBRACKET: Key = Key(0x1B);
    pub const ENTER: Key = Key(0x1C);
    pub const LCTRL: Key = Key(0x1D);
    pub const A: Key = Key(0x1E);
    pub const S: Key = Key(0x1F);
    pub const D: Key = Key(0x20);
    pub const F: Key = Key(0x21);
    pub const G: Key = Key(0x22);
    pub const H: Key = Key(0x23);
    pub const J: Key = Key(0x24);
    pub const K: Key = Key(0x25);
    pub const L: Key = Key(0x26);
    pub const SEMICOLON: Key = Key(0x27);
    pub const QUOTE: Key = Key(0x28);
    pub const GRAVE: Key = Key(0x29);
    pub const LSHIFT: Key = Key(0x2A);
    pub const BACKSLASH: Key = Key(0x2B);
    pub const Z: Key = Key(0x2C);
    pub const X: Key = Key(0x2D);
    pub const C: Key = Key(0x2E);
    pub const V: Key = Key(0x2F);
    pub const B: Key = Key(0x30);
    pub const N: Key = Key(0x31);
    pub const M: Key = Key(0x32);
    pub const COMMA: Key = Key(0x33);
    pub const PERIOD: Key = Key(0x34);
    pub const SLASH: Key = Key(0x35);
    pub const RSHIFT: Key = Key(0x36);
    pub const NUM_MULTIPLY: Key = Key(0x37);
    pub const LALT: Key = Key(0x38);
    pub const SPACE: Key = Key(0x39);
    pub const CAPSLOCK: Key = Key(0x3A);
    pub const F1: Key = Key(0x3B);
    pub const F2: Key = Key(0x3C);
    pub const F3: Key = Key(0x3D);
    pub const F4: Key = Key(0x3E);
    pub const F5: Key = Key(0x3F);
    pub const F6: Key = Key(0x40);
    pub const F7: Key = Key(0x41);
    pub const F8: Key = Key(0x42);
    pub const F9: Key = Key(0x43);
    pub const F10: Key = Key(0x44);
    pub const NUMLOCK: Key = Key(0x45);
    pub const SCROLLLOCK: Key = Key(0x46);
    pub const NUM7: Key = Key(0x47);
    pub const NUM8: Key = Key(0x48);
    pub const NUM9: Key = Key(0x49);
    pub const NUM_MINUS: Key = Key(0x4A);
    pub const NUM4: Key = Key(0x4B);
    pub const NUM5: Key = Key(0x4C);
    pub const NUM6: Key = Key(0x4D);
    pub const NUM_PLUS: Key = Key(0x4E);
    pub const NUM1: Key = Key(0x4F);
    pub const NUM2: Key = Key(0x50);
    pub const NUM3: Key = Key(0x51);
    pub const NUM0: Key = Key(0x52);
    pub const NUM_DECIMAL: Key = Key(0x53);
    pub const INTL_BACKSLASH: Key = Key(0x56);
    pub const F11: Key = Key(0x57);
    pub const F12: Key = Key(0x58);
    pub const NUM_EQUAL: Key = Key(0x59);
    pub const F13: Key = Key(0x64);
    pub const F14: Key = Key(0x65);
    pub const F15: Key = Key(0x66);
    pub const F16: Key = Key(0x67);
    pub const F17: Key = Key(0x68);
    pub const F18: Key = Key(0x69);
    pub const F19: Key = Key(0x6A);
    pub const NUM_ENTER: Key = Key::ext(0x1C);
    pub const RCTRL: Key = Key::ext(0x1D);
    pub const NUM_DIVIDE: Key = Key::ext(0x35);
    pub const PRINTSCREEN: Key = Key::ext(0x37);
    pub const RALT: Key = Key::ext(0x38);
    pub const HOME: Key = Key::ext(0x47);
    pub const UP: Key = Key::ext(0x48);
    pub const PAGEUP: Key = Key::ext(0x49);
    pub const LEFT: Key = Key::ext(0x4B);
    pub const RIGHT: Key = Key::ext(0x4D);
    pub const END: Key = Key::ext(0x4F);
    pub const DOWN: Key = Key::ext(0x50);
    pub const PAGEDOWN: Key = Key::ext(0x51);
    pub const INSERT: Key = Key::ext(0x52);
    pub const DELETE: Key = Key::ext(0x53);
    pub const LWIN: Key = Key::ext(0x5B);
    pub const RWIN: Key = Key::ext(0x5C);
    pub const MENU: Key = Key::ext(0x5D);

    /// The fn key (Apple vendor report 0x11, bit 0x10).
    pub const FN: Key = Key(0xF001);
    /// The Eject key (Apple vendor report 0x11, bit 0x08).
    pub const EJECT: Key = Key(0xF002);

    pub fn is_modifier(self) -> bool {
        self.mod_class().is_some()
    }

    /// Which Windows modifier this key is, if any.
    pub fn mod_class(self) -> Option<OutMod> {
        match self {
            Key::LCTRL | Key::RCTRL => Some(OutMod::Ctrl),
            Key::LALT | Key::RALT => Some(OutMod::Alt),
            Key::LSHIFT | Key::RSHIFT => Some(OutMod::Shift),
            Key::LWIN | Key::RWIN => Some(OutMod::Win),
            _ => None,
        }
    }

    /// Which Apple keycap modifier this physical key is, if any.
    pub fn phys_mod(self) -> Option<PhysMods> {
        match self {
            Key::LCTRL | Key::RCTRL => Some(PhysMods::CTRL),
            Key::LALT | Key::RALT => Some(PhysMods::OPT),
            Key::LSHIFT | Key::RSHIFT => Some(PhysMods::SHIFT),
            Key::LWIN | Key::RWIN => Some(PhysMods::CMD),
            Key::FN => Some(PhysMods::FN),
            _ => None,
        }
    }

    pub fn is_fkey(self) -> bool {
        matches!(self.0, 0x3B..=0x44 | 0x57 | 0x58)
    }

    pub fn parse(s: &str) -> Option<Key> {
        let t = s.trim();
        if let Some(hex) = t.strip_prefix("sc:").or_else(|| t.strip_prefix("SC:")) {
            return u16::from_str_radix(hex.trim_start_matches("0x"), 16).ok().map(Key);
        }
        NAMES
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(t))
            .map(|&(_, k)| k)
            .or_else(|| ALIASES.iter().find(|(n, _)| n.eq_ignore_ascii_case(t)).map(|&(_, k)| k))
    }

    pub fn name(self) -> String {
        NAMES
            .iter()
            .find(|&&(_, k)| k == self)
            .map(|(n, _)| n.to_string())
            .unwrap_or_else(|| format!("sc:{:04X}", self.0))
    }
}

impl fmt::Debug for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.name())
    }
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.name())
    }
}

/// Canonical names, used both for parsing and for display.
pub const NAMES: &[(&str, Key)] = &[
    ("Esc", Key::ESC),
    ("1", Key::N1),
    ("2", Key::N2),
    ("3", Key::N3),
    ("4", Key::N4),
    ("5", Key::N5),
    ("6", Key::N6),
    ("7", Key::N7),
    ("8", Key::N8),
    ("9", Key::N9),
    ("0", Key::N0),
    ("Minus", Key::MINUS),
    ("Equal", Key::EQUAL),
    ("Backspace", Key::BACKSPACE),
    ("Tab", Key::TAB),
    ("Q", Key::Q),
    ("W", Key::W),
    ("E", Key::E),
    ("R", Key::R),
    ("T", Key::T),
    ("Y", Key::Y),
    ("U", Key::U),
    ("I", Key::I),
    ("O", Key::O),
    ("P", Key::P),
    ("LBracket", Key::LBRACKET),
    ("RBracket", Key::RBRACKET),
    ("Enter", Key::ENTER),
    ("LCtrl", Key::LCTRL),
    ("A", Key::A),
    ("S", Key::S),
    ("D", Key::D),
    ("F", Key::F),
    ("G", Key::G),
    ("H", Key::H),
    ("J", Key::J),
    ("K", Key::K),
    ("L", Key::L),
    ("Semicolon", Key::SEMICOLON),
    ("Quote", Key::QUOTE),
    ("Grave", Key::GRAVE),
    ("LShift", Key::LSHIFT),
    ("Backslash", Key::BACKSLASH),
    ("Z", Key::Z),
    ("X", Key::X),
    ("C", Key::C),
    ("V", Key::V),
    ("B", Key::B),
    ("N", Key::N),
    ("M", Key::M),
    ("Comma", Key::COMMA),
    ("Period", Key::PERIOD),
    ("Slash", Key::SLASH),
    ("RShift", Key::RSHIFT),
    ("NumMultiply", Key::NUM_MULTIPLY),
    ("LAlt", Key::LALT),
    ("Space", Key::SPACE),
    ("CapsLock", Key::CAPSLOCK),
    ("F1", Key::F1),
    ("F2", Key::F2),
    ("F3", Key::F3),
    ("F4", Key::F4),
    ("F5", Key::F5),
    ("F6", Key::F6),
    ("F7", Key::F7),
    ("F8", Key::F8),
    ("F9", Key::F9),
    ("F10", Key::F10),
    ("F11", Key::F11),
    ("F12", Key::F12),
    ("F13", Key::F13),
    ("F14", Key::F14),
    ("F15", Key::F15),
    ("F16", Key::F16),
    ("F17", Key::F17),
    ("F18", Key::F18),
    ("F19", Key::F19),
    ("NumLock", Key::NUMLOCK),
    ("ScrollLock", Key::SCROLLLOCK),
    ("Num7", Key::NUM7),
    ("Num8", Key::NUM8),
    ("Num9", Key::NUM9),
    ("NumMinus", Key::NUM_MINUS),
    ("Num4", Key::NUM4),
    ("Num5", Key::NUM5),
    ("Num6", Key::NUM6),
    ("NumPlus", Key::NUM_PLUS),
    ("Num1", Key::NUM1),
    ("Num2", Key::NUM2),
    ("Num3", Key::NUM3),
    ("Num0", Key::NUM0),
    ("NumDecimal", Key::NUM_DECIMAL),
    ("NumEqual", Key::NUM_EQUAL),
    ("NumEnter", Key::NUM_ENTER),
    ("NumDivide", Key::NUM_DIVIDE),
    ("IntlBackslash", Key::INTL_BACKSLASH),
    ("RCtrl", Key::RCTRL),
    ("PrintScreen", Key::PRINTSCREEN),
    ("RAlt", Key::RALT),
    ("Home", Key::HOME),
    ("Up", Key::UP),
    ("PageUp", Key::PAGEUP),
    ("Left", Key::LEFT),
    ("Right", Key::RIGHT),
    ("End", Key::END),
    ("Down", Key::DOWN),
    ("PageDown", Key::PAGEDOWN),
    ("Insert", Key::INSERT),
    ("Delete", Key::DELETE),
    ("LWin", Key::LWIN),
    ("RWin", Key::RWIN),
    ("Menu", Key::MENU),
    ("Fn", Key::FN),
    ("Eject", Key::EJECT),
];

/// Extra spellings accepted when parsing.
const ALIASES: &[(&str, Key)] = &[
    ("Escape", Key::ESC),
    ("Return", Key::ENTER),
    ("Del", Key::DELETE),
    ("Ins", Key::INSERT),
    ("PgUp", Key::PAGEUP),
    ("PgDn", Key::PAGEDOWN),
    ("PrtSc", Key::PRINTSCREEN),
    ("ScrLk", Key::SCROLLLOCK),
    ("Caps", Key::CAPSLOCK),
    ("Apps", Key::MENU),
    ("LCmd", Key::LWIN),
    ("RCmd", Key::RWIN),
    ("LOption", Key::LALT),
    ("ROption", Key::RALT),
    ("Backtick", Key::GRAVE),
    ("Section", Key::INTL_BACKSLASH),
    ("`", Key::GRAVE),
    ("-", Key::MINUS),
    ("=", Key::EQUAL),
    ("[", Key::LBRACKET),
    ("]", Key::RBRACKET),
    (";", Key::SEMICOLON),
    ("'", Key::QUOTE),
    ("\\", Key::BACKSLASH),
    (",", Key::COMMA),
    (".", Key::PERIOD),
    ("/", Key::SLASH),
];

/// Windows modifier classes (what the OS sees).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum OutMod {
    Ctrl,
    Alt,
    Shift,
    Win,
}

impl OutMod {
    pub const ALL: [OutMod; 4] = [OutMod::Ctrl, OutMod::Alt, OutMod::Shift, OutMod::Win];

    /// The key injected when this modifier has to be pressed synthetically.
    pub fn canonical_key(self) -> Key {
        match self {
            OutMod::Ctrl => Key::LCTRL,
            OutMod::Alt => Key::LALT,
            OutMod::Shift => Key::LSHIFT,
            OutMod::Win => Key::LWIN,
        }
    }
}

/// Set of Windows modifiers in an output chord.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct OutMods(pub u8);

impl OutMods {
    pub const NONE: OutMods = OutMods(0);
    pub const CTRL: OutMods = OutMods(1);
    pub const ALT: OutMods = OutMods(2);
    pub const SHIFT: OutMods = OutMods(4);
    pub const WIN: OutMods = OutMods(8);

    pub fn bit(m: OutMod) -> OutMods {
        match m {
            OutMod::Ctrl => OutMods::CTRL,
            OutMod::Alt => OutMods::ALT,
            OutMod::Shift => OutMods::SHIFT,
            OutMod::Win => OutMods::WIN,
        }
    }
    pub fn has(self, m: OutMod) -> bool {
        self.0 & OutMods::bit(m).0 != 0
    }
    pub fn iter(self) -> impl Iterator<Item = OutMod> {
        OutMod::ALL.into_iter().filter(move |&m| self.has(m))
    }
}

impl std::ops::BitOr for OutMods {
    type Output = OutMods;
    fn bitor(self, o: OutMods) -> OutMods {
        OutMods(self.0 | o.0)
    }
}

impl fmt::Debug for OutMods {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let names: Vec<_> = self.iter().map(|m| format!("{m:?}")).collect();
        f.write_str(&names.join("+"))
    }
}

/// Set of Apple keycap modifiers the user physically holds (used to match rules).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct PhysMods(pub u8);

impl PhysMods {
    pub const NONE: PhysMods = PhysMods(0);
    pub const CMD: PhysMods = PhysMods(1);
    pub const OPT: PhysMods = PhysMods(2);
    pub const CTRL: PhysMods = PhysMods(4);
    pub const SHIFT: PhysMods = PhysMods(8);
    pub const FN: PhysMods = PhysMods(16);
    pub const ALL: PhysMods = PhysMods(31);

    pub fn contains(self, o: PhysMods) -> bool {
        self.0 & o.0 == o.0
    }
    pub fn intersects(self, o: PhysMods) -> bool {
        self.0 & o.0 != 0
    }
    pub fn minus(self, o: PhysMods) -> PhysMods {
        PhysMods(self.0 & !o.0)
    }
    pub fn count(self) -> u32 {
        self.0.count_ones()
    }
}

impl std::ops::BitOr for PhysMods {
    type Output = PhysMods;
    fn bitor(self, o: PhysMods) -> PhysMods {
        PhysMods(self.0 | o.0)
    }
}

impl fmt::Debug for PhysMods {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut v = Vec::new();
        for (bit, n) in [
            (PhysMods::FN, "Fn"),
            (PhysMods::CTRL, "Ctrl"),
            (PhysMods::OPT, "Opt"),
            (PhysMods::CMD, "Cmd"),
            (PhysMods::SHIFT, "Shift"),
        ] {
            if self.contains(bit) {
                v.push(n);
            }
        }
        f.write_str(&v.join("+"))
    }
}

/// Windows virtual-key codes the engine injects directly (no scan code equivalent).
pub mod vk {
    pub const PAUSE: u16 = 0x13;
    pub const BROWSER_BACK: u16 = 0xA6;
    pub const BROWSER_FORWARD: u16 = 0xA7;
    pub const VOLUME_MUTE: u16 = 0xAD;
    pub const VOLUME_DOWN: u16 = 0xAE;
    pub const VOLUME_UP: u16 = 0xAF;
    pub const MEDIA_NEXT: u16 = 0xB0;
    pub const MEDIA_PREV: u16 = 0xB1;
    pub const MEDIA_STOP: u16 = 0xB2;
    pub const MEDIA_PLAY_PAUSE: u16 = 0xB3;
    /// Unassigned VK used to keep a lone Alt/Win release from opening menus.
    pub const MASK: u16 = 0xE8;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_roundtrip() {
        for &(n, k) in NAMES {
            assert_eq!(Key::parse(n), Some(k), "{n}");
            assert_eq!(k.name(), n);
        }
        assert_eq!(Key::parse("pgdn"), Some(Key::PAGEDOWN));
        assert_eq!(Key::parse("sc:E053"), Some(Key::DELETE));
    }

    #[test]
    fn scan_codes() {
        assert_eq!(Key::from_scan(0x53, true), Key::DELETE);
        assert_eq!(Key::DELETE.scan_code(), 0x53);
        assert!(Key::DELETE.is_extended());
        assert!(!Key::NUM_DECIMAL.is_extended());
        assert!(Key::EJECT.is_virtual() && !Key::EJECT.is_extended());
    }
}
