//! Scenario tests for the engine. Each step is a physical event; the expectation is what
//! Windows receives: `pass` for the untouched original, otherwise the injected stream
//! written as `Key+` (down) / `Key-` (up), `vk:XX+/-` for virtual keys.

use alukeys_core::config::{Config, TopRow};
use alukeys_core::presets::Preset;
use alukeys_core::{Effect, Engine, Key, Out};

fn engine(cfg: Config) -> Engine {
    let (map, errors) = cfg.compile(true);
    assert!(errors.is_empty(), "{errors:?}");
    Engine::new(map)
}

fn mac() -> Engine {
    engine(Config::default())
}

fn fmt(out: &[Out]) -> String {
    out.iter()
        .map(|o| match o {
            Out::Key { key, down } => format!("{key}{}", if *down { "+" } else { "-" }),
            Out::Vk { vk, down } => format!("vk:{vk:02X}{}", if *down { "+" } else { "-" }),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Runs `steps` ("LWin+", "C-", …) and returns one line per step.
fn run(e: &mut Engine, steps: &str) -> Vec<String> {
    steps
        .split_whitespace()
        .map(|s| {
            let (name, down) = s.split_at(s.len() - 1);
            let key = Key::parse(name).unwrap_or_else(|| panic!("key {name}"));
            let r = e.handle(key, down == "+");
            let mut line = if r.pass { "pass".to_string() } else { fmt(&r.out) };
            for eff in r.effects {
                line.push_str(&format!(" !{eff:?}"));
            }
            line
        })
        .collect()
}

#[track_caller]
fn check(e: &mut Engine, steps: &str, expected: &[&str]) {
    let got = run(e, steps);
    assert_eq!(got, expected, "steps: {steps}");
}

#[test]
fn plain_keys_pass_through() {
    let mut e = mac();
    check(
        &mut e,
        "A+ A+ A- LShift+ B+ B- LShift-",
        &["pass", "pass", "pass", "pass", "pass", "pass", "pass"],
    );
}

#[test]
fn cmd_acts_as_ctrl() {
    let mut e = mac();
    check(&mut e, "LWin+ LWin+ C+ C- LWin-", &["LCtrl+", "", "pass", "pass", "LCtrl-"]);
}

#[test]
fn cmd_and_ctrl_share_lctrl() {
    let mut e = mac();
    // Physical Ctrl while Cmd holds LCtrl: Windows must not see Ctrl go up early.
    check(&mut e, "LWin+ LCtrl+ LWin- X+ X- LCtrl-", &["LCtrl+", "", "", "pass", "pass", "pass"]);
}

#[test]
fn cmd_q_closes_window() {
    let mut e = mac();
    check(&mut e, "LWin+ Q+ Q- LWin-", &["LCtrl+", "LCtrl- LAlt+ F4+ F4- LAlt- LCtrl+", "", "LCtrl-"]);
}

#[test]
fn cmd_tab_holds_alt() {
    let mut e = mac();
    check(
        &mut e,
        "LWin+ Tab+ Tab- Tab+ Tab- LShift+ Tab+ Tab- LShift- LWin-",
        &["LCtrl+", "LCtrl- LAlt+ Tab+ Tab-", "", "Tab+ Tab-", "", "pass", "Tab+ Tab-", "", "pass", "LAlt-"],
    );
}

#[test]
fn cmd_arrows_keep_shift() {
    let mut e = mac();
    check(
        &mut e,
        "LWin+ LShift+ Left+ Left+ Left- LShift- LWin-",
        &["LCtrl+", "pass", "LCtrl- Home+ Home- LCtrl+", "LCtrl- Home+ Home- LCtrl+", "", "pass", "LCtrl-"],
    );
    check(&mut e, "LWin+ Up+ Up- LWin-", &["LCtrl+", "Home+ Home-", "", "LCtrl-"]);
}

#[test]
fn option_word_jump_masks_alt_menu() {
    let mut e = mac();
    check(
        &mut e,
        "LAlt+ Left+ Left- LAlt-",
        &["pass", "vk:E8+ vk:E8- LAlt- LCtrl+ Left+ Left- LCtrl- LAlt+", "", "vk:E8+ vk:E8- LAlt-"],
    );
}

#[test]
fn option_tap_still_reaches_windows() {
    let mut e = mac();
    check(&mut e, "LAlt+ LAlt-", &["pass", "pass"]);
    check(&mut e, "LAlt+ F+ F- LAlt-", &["pass", "pass", "pass", "pass"]);
}

#[test]
fn screenshots() {
    let mut e = mac();
    check(
        &mut e,
        "LWin+ LShift+ 4+ 4- LShift- LWin-",
        &["LCtrl+", "pass", "LCtrl- LWin+ S+ S- LWin- LCtrl+", "", "pass", "LCtrl-"],
    );
    check(
        &mut e,
        "LWin+ LShift+ 3+ 3- LShift- LWin-",
        &["LCtrl+", "pass", "LCtrl- LShift- PrintScreen+ PrintScreen- LShift+ LCtrl+", "", "pass", "LCtrl-"],
    );
}

#[test]
fn lock_and_task_manager() {
    let mut e = mac();
    check(&mut e, "LCtrl+ LWin+ Q+ Q- LWin- LCtrl-", &["pass", "", " !Lock", "", "", "pass"]);
    check(
        &mut e,
        "LWin+ LAlt+ Esc+ Esc- LAlt- LWin-",
        &[
            "LCtrl+",
            "pass",
            "vk:E8+ vk:E8- LAlt- LShift+ Esc+ Esc- LShift- LAlt+",
            "",
            "vk:E8+ vk:E8- LAlt-",
            "LCtrl-",
        ],
    );
}

#[test]
fn fn_layer() {
    let mut e = mac();
    check(
        &mut e,
        "Fn+ Left+ Left+ Left- Backspace+ Backspace- Fn-",
        &["", "Home+", "Home+", "Home-", "Delete+", "Delete-", ""],
    );
    // fn released before the arrow: the arrow still releases as Home.
    check(&mut e, "Fn+ Right+ Fn- Right-", &["", "End+", "", "End-"]);
    // fn + Shift + arrow selects to line end.
    check(&mut e, "Fn+ LShift+ Right+ Right- LShift- Fn-", &["", "pass", "End+", "End-", "pass", ""]);
}

#[test]
fn eject_is_delete_with_repeat() {
    let mut e = mac();
    check(
        &mut e,
        "Eject+ Eject+ Eject-",
        &["Delete+ !StartRepeat(Eject)", "Delete+", "Delete- !StopRepeat(Eject)"],
    );
    check(
        &mut e,
        "Fn+ Eject+ Eject- Fn-",
        &["", "Insert+ !StartRepeat(Eject)", "Insert- !StopRepeat(Eject)", ""],
    );
}

#[test]
fn top_row_media_first() {
    let mut e = mac();
    check(&mut e, "F8+ F8-", &["vk:B3+ vk:B3-", ""]);
    check(&mut e, "F12+ F12+ F12-", &["vk:AF+ vk:AF-", "vk:AF+ vk:AF-", ""]);
    check(&mut e, "F1+ F1-", &[" !Brightness(-1)", ""]);
    check(&mut e, "Fn+ F8+ F8- Fn-", &["", "pass", "pass", ""]);
    // With a modifier the top row sends F-keys: Option+F4 is Alt+F4.
    check(&mut e, "LAlt+ F4+ F4- LAlt-", &["pass", "pass", "pass", "pass"]);
    // F5 has no media legend and stays F5.
    check(&mut e, "F5+ F5-", &["pass", "pass"]);
}

#[test]
fn fn_lock_toggles_top_row() {
    let mut e = mac();
    check(&mut e, "Fn+ Esc+ Esc- Fn-", &["", " !FnLock(true)", "", ""]);
    check(&mut e, "F8+ F8-", &["pass", "pass"]);
    check(&mut e, "Fn+ F8+ F8- Fn-", &["", "vk:B3+ vk:B3-", "", ""]);
}

#[test]
fn function_mode_from_config() {
    let mut cfg = Config::default();
    cfg.top_row.mode = TopRow::Function;
    let mut e = engine(cfg);
    check(&mut e, "F10+ F10-", &["pass", "pass"]);
    check(&mut e, "Fn+ F10+ F10- Fn-", &["", "vk:AD+ vk:AD-", "", ""]);
}

#[test]
fn fn_cmd_is_windows_key() {
    let mut e = mac();
    check(&mut e, "Fn+ LWin+ LWin- Fn-", &["", "pass", "pass", ""]);
    check(&mut e, "Fn+ LWin+ E+ E- LWin- Fn-", &["", "pass", "pass", "pass", "pass", ""]);
}

#[test]
fn cmd_space_switches_language_ctrl_space_opens_start() {
    let mut e = mac();
    check(
        &mut e,
        "LWin+ Space+ Space- LWin-",
        &["LCtrl+", "LCtrl- LWin+ Space+ Space- LWin- LCtrl+", "", "LCtrl-"],
    );
    check(&mut e, "LCtrl+ Space+ Space- LCtrl-", &["pass", "LCtrl- LWin+ LWin- LCtrl+", "", "pass"]);
}

#[test]
fn iso_swap() {
    let mut e = mac();
    check(&mut e, "IntlBackslash+ IntlBackslash-", &["Grave+", "Grave-"]);
    check(&mut e, "Grave+ Grave-", &["IntlBackslash+", "IntlBackslash-"]);
    let (map, _) = Config::default().compile(false);
    let mut ansi = Engine::new(map);
    check(&mut ansi, "Grave+ Grave-", &["pass", "pass"]);
}

#[test]
fn pc_preset_swaps_option_and_cmd() {
    let mut e = engine(Config { preset: Preset::Pc, ..Config::default() });
    check(&mut e, "LWin+ Tab+ Tab- LWin-", &["LAlt+", "pass", "pass", "LAlt-"]);
    check(&mut e, "LAlt+ LAlt-", &["LWin+", "LWin-"]);
    check(&mut e, "Fn+ Left+ Left- Fn-", &["", "Home+", "Home-", ""]);
}

#[test]
fn as_is_preset_leaves_modifiers() {
    let mut e = engine(Config { preset: Preset::AsIs, ..Config::default() });
    check(&mut e, "LWin+ Q+ Q- LWin-", &["pass", "pass", "pass", "pass"]);
}

#[test]
fn reset_releases_everything() {
    let mut e = mac();
    run(&mut e, "LWin+ LShift+ Eject+");
    let r = e.reset();
    assert_eq!(fmt(&r.out), "Delete- LCtrl- LShift-");
    assert_eq!(r.effects, vec![Effect::StopRepeat(Key::EJECT)]);
    // Releases after a reset are passed on and do not crash.
    check(&mut e, "LWin- LShift-", &["pass", "pass"]);
}

#[test]
fn paused_engine_passes_everything() {
    let mut e = mac();
    e.set_paused(true);
    check(&mut e, "LWin+ Q+ Q- LWin- Eject+ Eject-", &["pass", "pass", "pass", "pass", "", ""]);
}

#[test]
fn user_rules_and_disabled_groups() {
    let cfg = Config::from_toml(
        r#"
        disabled = ["cmd-q"]
        [remap]
        CapsLock = "Esc"
        [[rules]]
        trigger = "Cmd+Space"
        action = "Alt+LShift"
        "#,
    )
    .unwrap();
    let mut e = engine(cfg);
    check(&mut e, "LWin+ Q+ Q- LWin-", &["LCtrl+", "pass", "pass", "LCtrl-"]);
    check(&mut e, "CapsLock+ CapsLock-", &["Esc+", "Esc-"]);
    check(
        &mut e,
        "LWin+ Space+ Space- LWin-",
        &["LCtrl+", "LCtrl- LAlt+ LShift+ LShift- LAlt- LCtrl+", "", "LCtrl-"],
    );
}
