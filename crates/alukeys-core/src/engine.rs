//! The remapping state machine. Pure and platform independent: the Windows layer feeds
//! it physical key events and executes the returned output.
//!
//! Each physical key is resolved once, when it goes down, and the resolution is kept
//! until it goes up. Releasing keys in any order, or pressing fn mid-way, therefore never
//! leaves a stuck key.

use std::collections::HashMap;

use crate::action::{Action, Chord};
use crate::config::Keymap;
use crate::emitter::{Emitter, Out};
use crate::keys::{Key, OutMods, PhysMods};

/// Side effects the platform layer performs.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Effect {
    /// Change display brightness by this many steps.
    Brightness(i8),
    /// Top row mode changed; `true` means F1–F12 come first.
    FnLock(bool),
    /// Lock the workstation (Win+L cannot be injected).
    Lock,
    /// Generate auto-repeat for a key Windows does not repeat (Eject).
    StartRepeat(Key),
    StopRepeat(Key),
}

#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Response {
    /// Let the original event through untouched. Only true when `out` is empty.
    pub pass: bool,
    pub out: Vec<Out>,
    pub effects: Vec<Effect>,
}

#[derive(Clone, Debug)]
enum Held {
    /// Acts as `key` while held.
    Hold(Key),
    /// Runs the action on press and on every auto-repeat.
    Repeat(Action, PhysMods),
    /// Nothing happens.
    Swallow,
}

pub struct Engine {
    map: Keymap,
    functions_first: bool,
    held: HashMap<Key, Held>,
    em: Emitter,
    paused: bool,
}

impl Engine {
    pub fn new(map: Keymap) -> Engine {
        Engine {
            functions_first: map.functions_first,
            map,
            held: HashMap::new(),
            em: Emitter::default(),
            paused: false,
        }
    }

    /// Swaps in a new keymap, releasing anything held under the old one first.
    pub fn set_keymap(&mut self, map: Keymap) -> Response {
        let r = self.reset();
        self.functions_first = map.functions_first;
        self.map = map;
        r
    }

    pub fn functions_first(&self) -> bool {
        self.functions_first
    }

    pub fn set_functions_first(&mut self, on: bool) {
        self.functions_first = on;
    }

    pub fn paused(&self) -> bool {
        self.paused
    }

    pub fn set_paused(&mut self, paused: bool) -> Response {
        let r = self.reset();
        self.paused = paused;
        r
    }

    /// Releases every key this engine pressed. Call on sleep, lock, disconnect and exit.
    pub fn reset(&mut self) -> Response {
        let mut effects = Vec::new();
        for k in self.held.keys() {
            if k.is_virtual() {
                effects.push(Effect::StopRepeat(*k));
            }
        }
        self.held.clear();
        self.em.release_all();
        Response { pass: false, out: self.em.take(), effects }
    }

    /// Apple keycap modifiers physically held right now.
    pub fn phys_mods(&self) -> PhysMods {
        self.held.keys().filter_map(|k| k.phys_mod()).fold(PhysMods::NONE, |a, b| a | b)
    }

    /// Handles one physical event. `down` repeats arrive as further `down`s.
    pub fn handle(&mut self, key: Key, down: bool) -> Response {
        if self.paused {
            return Response { pass: !key.is_virtual(), ..Default::default() };
        }
        let mut effects = Vec::new();
        match (down, self.held.get(&key).cloned()) {
            (true, None) => {
                let mods = self.phys_mods();
                let held = self.press(key, mods, &mut effects);
                self.held.insert(key, held);
            }
            (true, Some(h)) => self.repeat(&h, &mut effects),
            (false, Some(h)) => {
                self.held.remove(&key);
                self.release(key, &h, &mut effects);
            }
            // Released a key pressed before we started, or one we never saw.
            (false, None) => {
                return Response { pass: !key.is_virtual(), ..Default::default() };
            }
        }
        let out = self.em.take();
        let pass = !key.is_virtual() && out == [Out::Key { key, down }];
        Response { pass, out: if pass { Vec::new() } else { out }, effects }
    }

    fn resolve(&self, key: Key, mods: PhysMods) -> (Action, PhysMods) {
        if key == Key::FN {
            return (Action::None, PhysMods::NONE);
        }
        let mut mods = mods;
        if key.is_fkey()
            && let Some(media) = self.map.media.get(&key)
        {
            let fn_held = mods.contains(PhysMods::FN);
            let others = mods.minus(PhysMods::FN) != PhysMods::NONE;
            let media_first = !self.functions_first;
            let use_media =
                if self.map.function_with_modifiers && others { false } else { media_first != fn_held };
            if use_media {
                return (media.clone(), PhysMods::FN);
            }
            mods = mods.minus(PhysMods::FN);
        }
        let best =
            self.map.rules.iter().filter(|r| r.matches(key, mods)).max_by_key(|r| r.trigger.mods.count());
        if let Some(r) = best {
            return (r.action.clone(), r.trigger.mods);
        }
        if let Some(a) = self.map.remap.get(&key) {
            return (a.clone(), PhysMods::NONE);
        }
        if key.is_virtual() {
            return (Action::None, PhysMods::NONE);
        }
        (Action::Key(key), PhysMods::NONE)
    }

    fn press(&mut self, key: Key, mods: PhysMods, effects: &mut Vec<Effect>) -> Held {
        let (action, consumed) = self.resolve(key, mods);
        let held = match action {
            Action::None => Held::Swallow,
            // A held remap is only safe when no emitted modifier had to be consumed;
            // otherwise behave like a tap of the bare key.
            Action::Key(t) if consumed.minus(PhysMods::FN) == PhysMods::NONE => {
                self.em.press(t);
                Held::Hold(t)
            }
            Action::Key(t) => Held::Repeat(Action::Chord(Chord { mods: OutMods::NONE, key: t }), consumed),
            Action::FnLockToggle => {
                self.functions_first = !self.functions_first;
                effects.push(Effect::FnLock(self.functions_first));
                Held::Swallow
            }
            Action::Lock => {
                effects.push(Effect::Lock);
                Held::Swallow
            }
            a => Held::Repeat(a, consumed),
        };
        if let Held::Repeat(a, consumed) = &held {
            self.run(a, *consumed, effects);
        }
        if key.is_virtual() && !matches!(held, Held::Swallow) {
            effects.push(Effect::StartRepeat(key));
        }
        held
    }

    fn repeat(&mut self, h: &Held, effects: &mut Vec<Effect>) {
        match h {
            Held::Hold(t) => self.em.repeat(*t),
            Held::Repeat(a, consumed) => self.run(a, *consumed, effects),
            Held::Swallow => {}
        }
    }

    fn release(&mut self, key: Key, h: &Held, effects: &mut Vec<Effect>) {
        if let Held::Hold(t) = h {
            self.em.release(*t);
        }
        if key.is_virtual() && !matches!(h, Held::Swallow) {
            effects.push(Effect::StopRepeat(key));
        }
    }

    fn run(&mut self, a: &Action, consumed: PhysMods, effects: &mut Vec<Effect>) {
        match a {
            Action::Chord(c) => self.chord(*c, consumed),
            Action::Sequence(v) => {
                for c in v {
                    self.chord(*c, consumed);
                }
            }
            Action::Vk(v) => self.em.vk_tap(*v),
            Action::Brightness(d) => effects.push(Effect::Brightness(*d)),
            Action::Switcher(c) => self.switcher(*c, consumed),
            Action::Key(k) => self.em.tap(*k),
            Action::Text(t) => self.text(t, consumed),
            Action::None | Action::FnLockToggle | Action::Lock => {}
        }
    }

    /// Types text with the modifiers that triggered it lifted, so apps see plain
    /// characters rather than Alt+something.
    ///
    /// Option+Shift is also the Windows "switch input language" hotkey, which fires when
    /// one of the two is released with nothing pressed in between; an unassigned key
    /// before lifting and after restoring keeps it from firing.
    fn text(&mut self, t: &str, consumed: PhysMods) {
        let consumed_keys = self.emitted_by(consumed);
        let to_lift: Vec<Key> =
            self.em.mods_down().into_iter().filter(|k| consumed_keys.contains(k)).collect();
        if !to_lift.is_empty() {
            self.em.vk_tap(crate::keys::vk::MASK);
        }
        let lifted: Vec<(Key, u8)> = to_lift.into_iter().map(|k| (k, self.em.lift(k))).collect();
        self.em.text(t);
        let restored = !lifted.is_empty();
        for (k, n) in lifted.into_iter().rev() {
            self.em.restore(k, n);
        }
        if restored {
            self.em.vk_tap(crate::keys::vk::MASK);
        }
    }

    /// Logical keys emitted by physical modifiers in `consumed`.
    fn emitted_by(&self, consumed: PhysMods) -> Vec<Key> {
        let mut v: Vec<Key> = self
            .held
            .iter()
            .filter(|(k, _)| k.phys_mod().is_some_and(|m| consumed.contains(m)))
            .filter_map(|(_, h)| match h {
                Held::Hold(t) if t.is_modifier() => Some(*t),
                _ => None,
            })
            .collect();
        v.sort();
        v.dedup();
        v
    }

    /// Taps `c` so that Windows sees exactly the chord's modifiers plus any modifiers
    /// the user holds that the rule did not consume, then restores the previous state.
    fn chord(&mut self, c: Chord, consumed: PhysMods) {
        let consumed_keys = self.emitted_by(consumed);
        let mut lifted = Vec::new();
        for k in self.em.mods_down() {
            let class = k.mod_class().expect("modifier");
            if consumed_keys.contains(&k) && !c.mods.has(class) {
                lifted.push((k, self.em.lift(k)));
            }
        }
        let mut pressed = Vec::new();
        for m in c.mods.iter() {
            if !self.em.class_down(m) {
                let k = m.canonical_key();
                self.em.press(k);
                pressed.push(k);
            }
        }
        self.em.tap(c.key);
        for k in pressed.into_iter().rev() {
            self.em.release(k);
        }
        for (k, n) in lifted.into_iter().rev() {
            self.em.restore(k, n);
        }
    }

    /// Rebinds the consumed physical modifier (Cmd) to emit the chord's modifier (Alt)
    /// until it is released, then taps the key. Repeated presses only tap the key.
    fn switcher(&mut self, c: Chord, consumed: PhysMods) {
        let target = c.mods.iter().next().map(|m| m.canonical_key());
        if let Some(target) = target {
            let phys: Vec<Key> = self
                .held
                .iter()
                .filter(|(k, h)| {
                    k.phys_mod().is_some_and(|m| consumed.contains(m))
                        && matches!(h, Held::Hold(t) if t.is_modifier())
                })
                .map(|(k, _)| *k)
                .collect();
            for pk in phys {
                let Some(Held::Hold(old)) = self.held.get(&pk).cloned() else {
                    continue;
                };
                if old != target {
                    self.em.release(old);
                    self.em.press(target);
                    self.em.mark_used();
                    self.held.insert(pk, Held::Hold(target));
                }
            }
            if !self.em.is_down(target) {
                self.em.press(target);
            }
        }
        self.em.tap(c.key);
    }
}
