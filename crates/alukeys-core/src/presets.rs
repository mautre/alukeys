//! Built-in layouts. Rules are grouped so the UI can switch whole behaviours on and off.

use serde::{Deserialize, Serialize};

use crate::action::{Action, Rule};
use crate::keys::Key;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Preset {
    /// Cmd works like Ctrl, plus macOS shortcuts translated to Windows.
    #[default]
    Mac,
    /// Option and Cmd swapped, giving the PC order Ctrl | Win | Alt.
    Pc,
    /// Modifiers untouched; only the fn layer, Eject and the top row.
    AsIs,
}

/// A rule group as shown in the settings UI.
pub struct Group {
    pub id: &'static str,
    pub title_en: &'static str,
    pub title_ru: &'static str,
}

pub const GROUPS: &[Group] = &[
    Group {
        id: "fn-nav",
        title_en: "fn + arrows: Home/End/PgUp/PgDn",
        title_ru: "fn + стрелки: Home/End/PgUp/PgDn",
    },
    Group {
        id: "fn-edit",
        title_en: "fn + Backspace/Enter: Delete/Insert",
        title_ru: "fn + Backspace/Enter: Delete/Insert",
    },
    Group {
        id: "fn-system",
        title_en: "fn + P/K/B: PrintScreen/ScrollLock/Pause",
        title_ru: "fn + P/K/B: PrintScreen/ScrollLock/Pause",
    },
    Group {
        id: "fn-lock", title_en: "fn + Esc toggles fn lock", title_ru: "fn + Esc переключает fn-lock"
    },
    Group {
        id: "fn-win",
        title_en: "fn + Cmd acts as the Windows key",
        title_ru: "fn + Cmd работает как клавиша Windows",
    },
    Group {
        id: "cmd-tab", title_en: "Cmd+Tab switches windows", title_ru: "Cmd+Tab переключает окна"
    },
    Group {
        id: "cmd-q",
        title_en: "Cmd+Q closes the window (Alt+F4)",
        title_ru: "Cmd+Q закрывает окно (Alt+F4)",
    },
    Group {
        id: "cmd-space",
        title_en: "Cmd+Space switches the input language",
        title_ru: "Cmd+Space переключает язык ввода",
    },
    Group {
        id: "ctrl-space",
        title_en: "Ctrl+Space opens Start search (like Spotlight)",
        title_ru: "Ctrl+Space открывает поиск в Пуске (как Spotlight)",
    },
    Group {
        id: "cmd-arrows",
        title_en: "Cmd + arrows: line and document start/end",
        title_ru: "Cmd + стрелки: начало/конец строки и документа",
    },
    Group {
        id: "cmd-backspace",
        title_en: "Cmd+Backspace deletes to line start",
        title_ru: "Cmd+Backspace удаляет до начала строки",
    },
    Group {
        id: "opt-words",
        title_en: "Option + arrows/Backspace work by word",
        title_ru: "Option + стрелки/Backspace работают по словам",
    },
    Group {
        id: "screenshots",
        title_en: "Cmd+Shift+3/4/5 take screenshots",
        title_ru: "Cmd+Shift+3/4/5 делают скриншоты",
    },
    Group {
        id: "cmd-m", title_en: "Cmd+M minimizes the window", title_ru: "Cmd+M сворачивает окно"
    },
    Group {
        id: "redo", title_en: "Cmd+Shift+Z is Redo (Ctrl+Y)", title_ru: "Cmd+Shift+Z — повтор (Ctrl+Y)"
    },
    Group {
        id: "browser-nav",
        title_en: "Cmd+[ and Cmd+] go back/forward",
        title_ru: "Cmd+[ и Cmd+] — назад/вперёд",
    },
    Group {
        id: "lock",
        title_en: "Ctrl+Cmd+Q locks the computer",
        title_ru: "Ctrl+Cmd+Q блокирует компьютер",
    },
    Group {
        id: "force-quit",
        title_en: "Cmd+Option+Esc opens Task Manager",
        title_ru: "Cmd+Option+Esc открывает Диспетчер задач",
    },
    Group {
        id: "typography",
        title_en: "Option types typographic characters: — – « » ₽ • ° º ≠ ± … © ™",
        title_ru: "Option печатает типографские символы: — – « » ₽ • ° º ≠ ± … © ™",
    },
    Group {
        id: "ctrl-desktops",
        title_en: "Ctrl + arrows switch virtual desktops",
        title_ru: "Ctrl + стрелки переключают рабочие столы",
    },
];

/// Rules shared by every preset: the fn layer.
fn fn_layer() -> Vec<Rule> {
    vec![
        Rule::new("fn-nav", "Fn+Left", "Home"),
        Rule::new("fn-nav", "Fn+Right", "End"),
        Rule::new("fn-nav", "Fn+Up", "PageUp"),
        Rule::new("fn-nav", "Fn+Down", "PageDown"),
        Rule::new("fn-edit", "Fn+Backspace", "Delete"),
        Rule::new("fn-edit", "Fn+Enter", "Insert"),
        Rule::new("fn-edit", "Fn+Eject", "Insert"),
        Rule::new("fn-system", "Fn+P", "PrintScreen"),
        Rule::new("fn-system", "Fn+K", "ScrollLock"),
        Rule::new("fn-system", "Fn+B", "pause"),
        Rule::new("fn-lock", "Fn+Esc", "fn_lock"),
    ]
}

fn mac_rules() -> Vec<Rule> {
    vec![
        Rule::new("fn-win", "Fn+LWin", "LWin"),
        Rule::new("fn-win", "Fn+RWin", "RWin"),
        Rule::new("cmd-tab", "Cmd+Tab", "switcher:Alt+Tab"),
        Rule::new("cmd-q", "Cmd+Q", "Alt+F4"),
        Rule::new("cmd-space", "Cmd+Space", "Win+Space"),
        Rule::new("ctrl-space", "Ctrl+Space", "tap:LWin"),
        Rule::new("cmd-arrows", "Cmd+Left", "tap:Home"),
        Rule::new("cmd-arrows", "Cmd+Right", "tap:End"),
        Rule::new("cmd-arrows", "Cmd+Up", "Ctrl+Home"),
        Rule::new("cmd-arrows", "Cmd+Down", "Ctrl+End"),
        Rule::new("cmd-backspace", "Cmd+Backspace", "Shift+Home, Backspace"),
        Rule::new("opt-words", "Opt+Left", "Ctrl+Left"),
        Rule::new("opt-words", "Opt+Right", "Ctrl+Right"),
        Rule::new("opt-words", "Opt+Backspace", "Ctrl+Backspace"),
        Rule::new("opt-words", "Opt+Fn+Backspace", "Ctrl+Delete"),
        Rule::new("screenshots", "Cmd+Shift+3", "tap:PrintScreen"),
        Rule::new("screenshots", "Cmd+Shift+4", "Win+Shift+S"),
        Rule::new("screenshots", "Cmd+Shift+5", "Win+Shift+R"),
        Rule::new("cmd-m", "Cmd+M", "Win+Down"),
        Rule::new("redo", "Cmd+Shift+Z", "Ctrl+Y"),
        Rule::new("browser-nav", "Cmd+LBracket", "Alt+Left"),
        Rule::new("browser-nav", "Cmd+RBracket", "Alt+Right"),
        Rule::new("lock", "Ctrl+Cmd+Q", "lock"),
        Rule::new("force-quit", "Cmd+Opt+Esc", "Ctrl+Shift+Esc"),
        // Option layer for typography, following the macOS US layout where it has one.
        Rule::new("typography", "Opt+Minus", "text:–"),
        Rule::new("typography", "Opt+Shift+Minus", "text:—"),
        Rule::new("typography", "Opt+Backslash", "text:«"),
        Rule::new("typography", "Opt+Shift+Backslash", "text:»"),
        Rule::new("typography", "Opt+4", "text:₽"),
        Rule::new("typography", "Opt+8", "text:•"),
        Rule::new("typography", "Opt+Shift+8", "text:°"),
        Rule::new("typography", "Opt+0", "text:º"),
        Rule::new("typography", "Opt+Equal", "text:≠"),
        Rule::new("typography", "Opt+Shift+Equal", "text:±"),
        Rule::new("typography", "Opt+Semicolon", "text:…"),
        Rule::new("typography", "Opt+G", "text:©"),
        Rule::new("typography", "Opt+2", "text:™"),
        Rule::new("typography", "Opt+Shift+2", "text:€"),
        Rule::new("typography", "Opt+LBracket", "text:“"),
        Rule::new("typography", "Opt+Shift+LBracket", "text:”"),
        Rule::new("typography", "Opt+RBracket", "text:‘"),
        Rule::new("typography", "Opt+Shift+RBracket", "text:’"),
        Rule::new("typography", "Opt+Comma", "text:„"),
        Rule::new("typography", "Opt+X", "text:×"),
        Rule::new("typography", "Opt+Slash", "text:÷"),
        Rule::new("ctrl-desktops", "Ctrl+Left", "Win+Ctrl+Left"),
        Rule::new("ctrl-desktops", "Ctrl+Right", "Win+Ctrl+Right"),
        Rule::new("ctrl-desktops", "Ctrl+Up", "Win+Tab"),
    ]
}

/// Built-in rules for a preset.
pub fn rules(preset: Preset) -> Vec<Rule> {
    let mut r = fn_layer();
    if preset == Preset::Mac {
        r.extend(mac_rules());
    }
    r
}

/// Built-in single-key remaps for a preset (physical key → action).
pub fn remaps(preset: Preset) -> Vec<(Key, Action)> {
    let mut v = vec![(Key::EJECT, Action::Key(Key::DELETE))];
    match preset {
        Preset::Mac => {
            v.push((Key::LWIN, Action::Key(Key::LCTRL)));
            v.push((Key::RWIN, Action::Key(Key::LCTRL)));
        }
        Preset::Pc => {
            v.push((Key::LWIN, Action::Key(Key::LALT)));
            v.push((Key::LALT, Action::Key(Key::LWIN)));
            v.push((Key::RWIN, Action::Key(Key::RALT)));
            v.push((Key::RALT, Action::Key(Key::RWIN)));
        }
        Preset::AsIs => {}
    }
    v
}

/// Top row media actions as printed on the 2009/2011 aluminium keyboards.
pub fn media_row() -> Vec<(Key, Action)> {
    [
        (Key::F1, "brightness_down"),
        (Key::F2, "brightness_up"),
        (Key::F3, "Win+Tab"),
        (Key::F4, "tap:LWin"),
        (Key::F7, "prev_track"),
        (Key::F8, "play_pause"),
        (Key::F9, "next_track"),
        (Key::F10, "mute"),
        (Key::F11, "volume_down"),
        (Key::F12, "volume_up"),
    ]
    .into_iter()
    .map(|(k, a)| (k, Action::parse(a).expect("built-in action")))
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_rule_group_is_described() {
        for p in [Preset::Mac, Preset::Pc, Preset::AsIs] {
            for r in rules(p) {
                assert!(GROUPS.iter().any(|g| g.id == r.group), "{}", r.group);
            }
        }
    }
}
