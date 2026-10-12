<img src="crates/alukeys/assets/icon-256.png" width="96" alt="AluKeys icon" align="right">

# AluKeys

Free software that makes Apple keyboards feel at home on Windows 10/11. No subscription, no telemetry, no kernel driver.

[Русская версия](README.ru.md)

Built for the Apple Wireless Keyboard A1314 (the aluminium one from the 2009–2011 iMacs); A1255 and the wired A1242/A1243 should work too.

## Features

- **fn works.** fn + arrows give Home/End/PgUp/PgDn, fn + Backspace is Delete, fn + Enter is Insert, fn + P/K/B are PrintScreen/ScrollLock/Pause.
- **Eject works** as Delete with auto-repeat; fn + Eject is Insert.
- **Mac-style top row:**
  - F1/F2 change brightness, including external monitors over DDC/CI;
  - F3 opens Task View, F4 opens Start;
  - F7–F9 control media, F10–F12 change the volume with the native Windows flyout.
  - fn + F-key sends F1–F12; fn + Esc toggles fn lock.
  - While Ctrl, Option, Cmd or Shift is held, the row always sends F-keys, so Alt+F4 and Ctrl+F5 need no fn.
- **Mac-style shortcuts (default).** ⌘ acts as Ctrl, so ⌘C, ⌘V, ⌘Z, ⌘T, ⌘W, ⌘-click and ⌘-scroll just work. In addition:

  | Shortcut | Does |
  |---|---|
  | ⌘Tab | switches windows |
  | ⌘Q | closes the window |
  | ⌘Space | switches the input language |
  | ⌃Space | opens Start search (like Spotlight) |
  | ⌘←/→ | line start/end |
  | ⌥←/→ | word left/right |
  | ⌥⌫ | deletes a word |
  | ⌘⇧3 / ⌘⇧4 | screenshot of the screen / an area |
  | ⌃⌘Q | locks the computer |
  | ⌘⌥Esc | opens Task Manager |
  | ⌃←/→ | switches virtual desktops |

  - fn + ⌘ is the Windows key.
  - Every rule can be turned off.
- **Mac-style typography on ⌥.** Characters are typed in any layout and any app; the set can be changed or turned off: ⌥- –, ⌥⇧- —, ⌥\\ «, ⌥⇧\\ », ⌥4 ₽, ⌥8 •, ⌥⇧8 °, ⌥0 º, ⌥= ≠, ⌥⇧= ±, ⌥; …, ⌥2 ™, ⌥⇧2 €, ⌥[ “, ⌥⇧[ ”, ⌥] ‘, ⌥⇧] ’, ⌥, „, ⌥G ©, ⌥X ×, ⌥/ ÷.
- **Modifier presets.** Besides "Like a Mac" there are "Like a PC" (⌥ and ⌘ swapped, giving Ctrl | Win | Alt) and "Unchanged".
- **§ and ` keys.** On ISO keyboards they are swapped back automatically, as macOS does.
- **Battery.** If the keyboard reports its level, it shows in the tray, with notifications at 20/10/5 %. The A1314 does not on Windows: it only answers a request that the Windows HID driver does not pass through.
- **Settings window:**
  - a map of your exact keyboard with the characters of your installed layouts;
  - layers (none / fn / ⌘ / ⌥ / ⌃) that follow the modifier you hold;
  - a live "pressed → Windows received" tester;
  - custom shortcuts and a first-run wizard.
- **Admin windows.** Starting as administrator (one UAC prompt, ever) makes remapping work in Task Manager and elevated terminals too.
- **Safety.** Nothing gets stuck: keys are released on lock, sleep, keyboard disconnect and pause.

## Install

1. Download `AluKeys-x64.zip` (or `-arm64`) from Releases and unpack it, e.g. to `%LOCALAPPDATA%\Programs\AluKeys`.
2. Pair the keyboard in Settings → Bluetooth & devices.
3. Run `alukeys.exe`. The first-run wizard opens.

The binaries are not signed yet, so SmartScreen may warn: More info → Run anyway.

For a portable setup, put an empty file named `portable` next to the exe.

## Troubleshooting

Run `alukeys-diag.exe`, follow the prompts and attach `alukeys-diag-report.txt` to an issue. The app log is `%APPDATA%\AluKeys\alukeys.log`.

- **fn or Eject do nothing.** Make sure Boot Camp or Magic Utilities drivers are not installed; they grab these keys first.
- **Brightness does not change.** The monitor needs DDC/CI, which is usually switched on in its on-screen menu. Laptop panels are not supported yet.
- **Nothing works on the sign-in screen.** Windows does not run apps there.

## Configuration

Everything is available in the settings window (click the tray icon). Settings live in `%APPDATA%\AluKeys\config.toml`; you can edit the file by hand and changes apply immediately.

```toml
preset = "mac"              # mac | pc | asis
iso_swap = "auto"           # auto | on | off
only_with_apple = true
disabled = ["ctrl-desktops"] # rule groups switched off

[top_row]
mode = "media"              # media | function
function_with_modifiers = true
[top_row.keys]
F5 = "media_stop"

[remap]                     # key → action
CapsLock = "LCtrl"

[[rules]]                   # custom shortcuts (Apple modifiers)
trigger = "Cmd+Shift+T"
action = "Ctrl+Shift+T"

[battery]
warn_at = [20, 10, 5]
```

**Actions:**
- `Home` — act as another key;
- `Ctrl+Shift+Esc` — tap a shortcut;
- `tap:LWin` — tap a single key;
- `Shift+Home, Backspace` — a sequence of shortcuts;
- `volume_up`, `volume_down`, `mute`, `play_pause`, `next_track`, `prev_track`, `media_stop`;
- `brightness_up`, `brightness_down`;
- `browser_back`, `browser_forward`, `pause`;
- `fn_lock`, `lock`, `none`;
- `switcher:Alt+Tab`;
- `text:—` — type characters;
- `vk:0x13` — a raw virtual key.

**Trigger modifiers:** `Fn`, `Ctrl`, `Opt`, `Cmd`, `Shift`.

## How it works

One executable, no driver:

- A `WH_KEYBOARD_LL` hook rewrites keystrokes (injected by scan code, so shortcuts work with any layout).
- The keyboard sends fn and Eject in a separate vendor HID report, which AluKeys reads through Raw Input.
- The battery level comes from the HID Battery Strength report when the keyboard sends it.
- The settings window is WebView2. It is created on demand and freed on close, so the background process stays at a few megabytes.

Layout:
- `crates/alukeys-core` — the platform-independent engine (presets, rules, state machine) with tests;
- `crates/alukeys` — the Windows layer (hook, raw input, tray, OSD, brightness, battery, settings window);
- `crates/alukeys-diag` — the diagnostic tool.

Build on Windows, or cross-compile from macOS/Linux with [cargo-xwin](https://github.com/rust-cross/cargo-xwin):

```bash
cargo test -p alukeys-core
cargo build --release -p alukeys --target x86_64-pc-windows-msvc
```

## Licence

MIT. Not affiliated with Apple Inc. Apple, Mac and Magic Keyboard are trademarks of Apple Inc.
