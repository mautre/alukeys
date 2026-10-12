//! Tracks what Windows believes is pressed and produces the output event stream.
//!
//! Several physical keys may emit the same logical key (Cmd and Ctrl both emit LCtrl in
//! the Mac preset), so logical keys are reference counted: Windows sees a press when the
//! first holder goes down and a release when the last one goes up.
//!
//! Alt and Win get special care. Windows opens the menu bar (Alt) or Start (Win) when the
//! modifier is released with nothing pressed since it went down. That is right for a
//! deliberate tap, but wrong when the modifier was part of a translated shortcut. Then the
//! emitter slips an unassigned key in before the release, as AutoHotkey does.

use std::collections::HashMap;

use crate::keys::{Key, OutMod, vk};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Out {
    /// Inject by scan code (layout independent).
    Key { key: Key, down: bool },
    /// Inject by virtual key (media keys, Pause, the menu mask).
    Vk { vk: u16, down: bool },
}

#[derive(Clone, Copy, Default, Debug)]
struct MenuGuard {
    /// Another key went down since this Alt/Win was pressed.
    keys_since: bool,
    /// The modifier took part in a shortcut, so a bare release must not open a menu.
    used: bool,
}

#[derive(Default, Debug)]
pub struct Emitter {
    count: HashMap<Key, u8>,
    guards: HashMap<Key, MenuGuard>,
    pub out: Vec<Out>,
}

fn guarded(k: Key) -> bool {
    matches!(k.mod_class(), Some(OutMod::Alt | OutMod::Win))
}

impl Emitter {
    pub fn is_down(&self, k: Key) -> bool {
        self.count.get(&k).copied().unwrap_or(0) > 0
    }

    /// Logical modifier keys currently down.
    pub fn mods_down(&self) -> Vec<Key> {
        let mut v: Vec<Key> =
            self.count.iter().filter(|(k, c)| **c > 0 && k.is_modifier()).map(|(k, _)| *k).collect();
        v.sort();
        v
    }

    pub fn class_down(&self, m: OutMod) -> bool {
        self.mods_down().iter().any(|k| k.mod_class() == Some(m))
    }

    fn raw(&mut self, key: Key, down: bool) {
        if down && !key.is_modifier() {
            for g in self.guards.values_mut() {
                g.keys_since = true;
                g.used = true;
            }
        }
        self.out.push(Out::Key { key, down });
    }

    pub fn vk_tap(&mut self, v: u16) {
        for g in self.guards.values_mut() {
            g.keys_since = true;
            g.used = true;
        }
        self.out.push(Out::Vk { vk: v, down: true });
        self.out.push(Out::Vk { vk: v, down: false });
    }

    fn emit_up(&mut self, key: Key) {
        if let Some(g) = self.guards.remove(&key)
            && g.used
            && !g.keys_since
        {
            self.out.push(Out::Vk { vk: vk::MASK, down: true });
            self.out.push(Out::Vk { vk: vk::MASK, down: false });
        }
        self.raw(key, false);
    }

    fn emit_down(&mut self, key: Key, used: bool) {
        if guarded(key) {
            self.guards.insert(key, MenuGuard { keys_since: false, used });
        }
        self.raw(key, true);
    }

    /// A holder presses `key`.
    pub fn press(&mut self, key: Key) {
        let c = self.count.entry(key).or_insert(0);
        *c += 1;
        if *c == 1 {
            self.emit_down(key, false);
        }
    }

    /// Auto-repeat of a held key (modifiers do not repeat).
    pub fn repeat(&mut self, key: Key) {
        if !key.is_modifier() {
            self.raw(key, true);
        }
    }

    /// A holder releases `key`.
    pub fn release(&mut self, key: Key) {
        let Some(c) = self.count.get_mut(&key) else {
            return;
        };
        if *c == 0 {
            return;
        }
        *c -= 1;
        if *c == 0 {
            self.count.remove(&key);
            self.emit_up(key);
        }
    }

    pub fn tap(&mut self, key: Key) {
        self.raw(key, true);
        self.raw(key, false);
    }

    /// Temporarily lifts a modifier for a translated shortcut; returns its holder count.
    pub fn lift(&mut self, key: Key) -> u8 {
        let c = self.count.remove(&key).unwrap_or(0);
        if c > 0 {
            if let Some(g) = self.guards.get_mut(&key) {
                g.used = true;
            }
            self.emit_up(key);
        }
        c
    }

    /// Puts back a modifier removed by [`lift`](Self::lift).
    pub fn restore(&mut self, key: Key, count: u8) {
        if count > 0 {
            self.count.insert(key, count);
            self.emit_down(key, true);
        }
    }

    /// Marks currently held Alt/Win as used, e.g. when switching their role.
    pub fn mark_used(&mut self) {
        for g in self.guards.values_mut() {
            g.used = true;
        }
    }

    /// Releases everything (pause, sleep, keyboard disconnect).
    pub fn release_all(&mut self) {
        let mut keys: Vec<Key> = self.count.keys().copied().collect();
        keys.sort();
        // Non-modifiers first, then modifiers, so no stray shortcut fires on the way.
        keys.sort_by_key(|k| k.is_modifier());
        self.count.clear();
        for k in keys {
            self.guards.remove(&k);
            self.raw(k, false);
        }
    }

    pub fn take(&mut self) -> Vec<Out> {
        std::mem::take(&mut self.out)
    }
}
