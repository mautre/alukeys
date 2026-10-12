//! The UI thread: tray icon and menu, notifications, OSD, config file watching, and
//! reacting to sleep/lock. All remapping happens on the engine thread.

use std::cell::RefCell;
use std::mem::zeroed;
use std::ptr::null_mut;
use std::time::SystemTime;

use alukeys_core::Config;
use alukeys_core::config::TopRow;
use alukeys_core::presets::Preset;
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::RemoteDesktop::{NOTIFY_FOR_THIS_SESSION, WTSRegisterSessionNotification};
use windows_sys::Win32::System::Shutdown::LockWorkStation;
use windows_sys::Win32::UI::Shell::ShellExecuteW;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use std::sync::atomic::Ordering;

use serde_json::Value;

use crate::autostart::{self, Mode};
use crate::osd::{self, glyph};
use crate::settings_window::{self, WM_SETTINGS_CLOSED, WM_SETTINGS_IPC};
use crate::shared::*;
use crate::tray::{self, Look};
use crate::util::{Hwnd, tr, wide};
use crate::worker::{self, Job};
use crate::{engine_thread, log, settings, ui_state};

const TIMER_CONFIG: usize = 1;
const TIMER_BATTERY: usize = 2;
const TIMER_BATTERY_SOON: usize = 3;
const BATTERY_POLL_MS: u32 = 10 * 60 * 1000;

/// Posted by a second instance to bring up the settings.
pub const WM_SHOW_MENU: u32 = WM_APP + 30;
/// Opens the settings window; wParam 1 starts the first-run wizard.
const WM_OPEN_SETTINGS: u32 = WM_APP + 31;

const ID_TOP_MEDIA: usize = 110;
const ID_TOP_FUNCTION: usize = 111;
const ID_PRESET_MAC: usize = 121;
const ID_PRESET_PC: usize = 122;
const ID_PRESET_ASIS: usize = 123;
const ID_PAUSE: usize = 130;
const ID_SETTINGS: usize = 135;
const ID_EDIT_CONFIG: usize = 140;
const ID_OPEN_FOLDER: usize = 141;
const ID_AUTO_OFF: usize = 151;
const ID_AUTO_NORMAL: usize = 152;
const ID_AUTO_ELEVATED: usize = 153;
const ID_ELEVATE: usize = 155;
const ID_ABOUT: usize = 160;
const ID_LOG_KEYS: usize = 161;
const ID_EXIT: usize = 199;

struct App {
    hwnd: HWND,
    config: Config,
    /// mtime of the config as we last read or wrote it.
    config_seen: Option<SystemTime>,
    icon: HICON,
    autostart: Mode,
    elevated: bool,
    /// Lowest battery threshold already announced (101 = none).
    battery_warned: u8,
    was_connected: bool,
    iso: bool,
    ddc_warned: bool,
    taskbar_created: u32,
}

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
}

fn with<R>(f: impl FnOnce(&mut App) -> R) -> Option<R> {
    APP.with(|a| a.try_borrow_mut().ok().and_then(|mut a| a.as_mut().map(f)))
}

fn engine_cmd(cmd: usize, arg: isize) {
    let e = shared().engine;
    unsafe { PostMessageW(e.get(), WM_ENGINE_CMD, cmd, arg) };
}

pub fn run(config: Config, config_error: Option<String>) {
    unsafe {
        let hinst = GetModuleHandleW(null_mut());
        let class = wide("AluKeysTray");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wnd_proc),
            hInstance: hinst,
            lpszClassName: class.as_ptr(),
            ..zeroed()
        };
        RegisterClassW(&wc);
        // A hidden top-level window (not message-only) so it receives broadcasts
        // such as TaskbarCreated and power events.
        let hwnd = CreateWindowExW(
            0,
            class.as_ptr(),
            class.as_ptr(),
            0,
            0,
            0,
            0,
            0,
            null_mut(),
            null_mut(),
            hinst,
            null_mut(),
        );
        {
            let mut sh = shared();
            sh.ui = Hwnd(hwnd as usize);
            sh.only_with_apple = config.only_with_apple;
        }
        osd::init();
        worker::start();

        let (keymap, errors) = config.compile(false);
        for e in &errors {
            log!("config: {e}");
        }
        let engine = engine_thread::spawn(keymap);

        let iso = engine_thread::is_iso();
        let elevated = autostart::is_elevated();
        let icon = tray::make_icon(Look::Normal);
        tray::add(hwnd, icon, "AluKeys");
        WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_THIS_SESSION);
        SetTimer(hwnd, TIMER_CONFIG, 1000, None);
        SetTimer(hwnd, TIMER_BATTERY, BATTERY_POLL_MS, None);

        APP.with(|a| {
            *a.borrow_mut() = Some(App {
                hwnd,
                config_seen: settings::modified(),
                config,
                icon,
                autostart: autostart::current(),
                elevated,
                battery_warned: 101,
                was_connected: shared().device.connected,
                iso: false,
                ddc_warned: false,
                taskbar_created: RegisterWindowMessageW(wide("TaskbarCreated").as_ptr()),
            })
        });
        // When elevated, still accept these from Explorer and from a non-elevated second copy.
        if let Some(tc) = with(|a| a.taskbar_created) {
            ChangeWindowMessageFilterEx(hwnd, tc, MSGFLT_ALLOW, null_mut());
        }
        ChangeWindowMessageFilterEx(hwnd, WM_SHOW_MENU, MSGFLT_ALLOW, null_mut());
        // Compile again now that the engine thread has detected the model (ISO swap).
        with(|a| {
            a.iso = !iso; // force the reload below
            a.on_device_changed(true);
        });
        if let Some(e) = config_error.or_else(|| errors.first().cloned()) {
            tray::notify(hwnd, tr("Ошибка в файле настроек", "Settings file error"), &e, true);
        }
        if with(|a| !a.config.ui.first_run_done) == Some(true) {
            PostMessageW(hwnd, WM_OPEN_SETTINGS, 1, 0);
        }

        let mut msg: MSG = zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        // CMD_QUIT is on its way; the engine releases held keys before it stops.
        let _ = engine.join();
    }
}

impl App {
    fn tooltip(&self) -> String {
        let sh = shared();
        let mut t = String::from("AluKeys");
        if sh.paused {
            t.push_str(tr(" — пауза", " — paused"));
        } else if !sh.device.connected {
            t.push_str(tr(" — клавиатура не подключена", " — keyboard not connected"));
        } else if let Some(b) = sh.battery {
            t.push_str(&format!(" — {}: {b}%", tr("батарея", "battery")));
        }
        t
    }

    fn refresh_tray(&mut self) {
        let look = {
            let sh = shared();
            let low = sh.battery.is_some_and(|b| self.config.battery.warn_at.iter().any(|&t| b <= t));
            if sh.paused || !sh.device.connected {
                Look::Dimmed
            } else if low {
                Look::LowBattery
            } else {
                Look::Normal
            }
        };
        let old = self.icon;
        self.icon = tray::make_icon(look);
        tray::update(self.hwnd, self.icon, &self.tooltip());
        tray::destroy_icon(old);
    }

    /// Recompiles the keymap and hands it to the engine thread.
    fn push_keymap(&mut self) {
        let (keymap, errors) = self.config.compile(self.iso);
        for e in &errors {
            log!("config: {e}");
        }
        {
            let mut sh = shared();
            sh.pending_keymap = Some(keymap);
            sh.only_with_apple = self.config.only_with_apple;
        }
        engine_cmd(CMD_RELOAD_KEYMAP, 0);
        crate::util::set_language(&self.config.ui.language);
    }

    fn save_config(&mut self) {
        settings::save(&self.config);
        self.config_seen = settings::modified();
        self.send_state();
    }

    fn send_state(&self) {
        if !settings_window::is_open() {
            return;
        }
        let (_, errors) = self.config.compile(self.iso);
        let ctx = ui_state::Context {
            config: &self.config,
            iso: self.iso,
            autostart: self.autostart,
            elevated: self.elevated,
            errors,
        };
        settings_window::send(&ui_state::state(&ctx));
    }

    /// Applies a config edited in the settings window.
    fn apply_config(&mut self, c: Config) {
        if c == self.config {
            return;
        }
        let ff = c.top_row.mode == TopRow::Function;
        self.config = c;
        self.save_config();
        self.push_keymap();
        engine_cmd(CMD_SET_FUNCTIONS_FIRST, ff as isize);
        self.refresh_tray();
    }

    fn on_ipc(&mut self, msg: &str) {
        let Ok(v) = serde_json::from_str::<Value>(msg) else {
            return;
        };
        match v["cmd"].as_str().unwrap_or("") {
            "ready" => self.send_state(),
            "set_config" => match serde_json::from_value::<Config>(v["config"].clone()) {
                Ok(c) => self.apply_config(c),
                Err(e) => log!("ui: bad config from page: {e}"),
            },
            "set_paused" => {
                self.set_paused(v["value"].as_bool() == Some(true));
                self.send_state();
            }
            "set_autostart" => {
                let id = match v["mode"].as_str() {
                    Some("elevated") => ID_AUTO_ELEVATED,
                    Some("normal") => ID_AUTO_NORMAL,
                    _ => ID_AUTO_OFF,
                };
                self.on_command(id);
            }
            "reset_all" => {
                let ui = self.config.ui.clone();
                self.apply_config(Config { ui, ..Config::default() });
            }
            "relaunch_elevated" => self.on_command(ID_ELEVATE),
            "open_folder" => self.on_command(ID_OPEN_FOLDER),
            "edit_file" => self.on_command(ID_EDIT_CONFIG),
            "open_bt" => open("ms-settings:bluetooth", ""),
            "test_osd" => {
                osd::show(glyph::BRIGHTNESS, tr("Так выглядит индикатор", "This is the indicator"), Some(60))
            }
            "close" => settings_window::close(),
            _ => {}
        }
    }

    fn on_device_changed(&mut self, startup: bool) {
        let iso = engine_thread::is_iso();
        if iso != self.iso {
            self.iso = iso;
            self.push_keymap();
        }
        let (connected, name) = {
            let sh = shared();
            let name =
                sh.device.model.map(|m| m.name.to_string()).unwrap_or_else(|| sh.device.product.clone());
            (sh.device.connected, name)
        };
        if connected != self.was_connected && !startup {
            let text = if connected {
                tr("Клавиатура подключена", "Keyboard connected")
            } else {
                tr("Клавиатура отключена", "Keyboard disconnected")
            };
            if self.config.ui.osd {
                osd::show(glyph::KEYBOARD, text, None);
            }
            log!("ui: {name}: {text}");
        }
        if connected {
            unsafe { SetTimer(self.hwnd, TIMER_BATTERY_SOON, 3000, None) };
            if name.is_empty() {
                worker::submit(Job::Product);
            }
        } else {
            shared().battery = None;
        }
        self.was_connected = connected;
        self.refresh_tray();
        self.send_state();
    }

    fn on_battery(&mut self, level: u8) {
        let changed = shared().battery.replace(level) != Some(level);
        let mut levels = self.config.battery.warn_at.clone();
        levels.sort_unstable_by(|a, b| b.cmp(a));
        if let Some(&top) = levels.first()
            && level > top.saturating_add(5)
        {
            self.battery_warned = 101;
        }
        for t in levels {
            if level <= t && self.battery_warned > t {
                self.battery_warned = t;
                let text = format!(
                    "{} {level}%. {}",
                    tr("Заряд клавиатуры", "Keyboard battery at"),
                    tr("Пора заменить батарейки.", "Time to replace the batteries.")
                );
                tray::notify(
                    self.hwnd,
                    tr("Батарея клавиатуры садится", "Keyboard battery low"),
                    &text,
                    true,
                );
            }
        }
        if changed {
            self.refresh_tray();
            self.send_state();
        }
    }

    fn set_fn_lock(&mut self, functions_first: bool, announce: bool) {
        self.config.top_row.mode = if functions_first { TopRow::Function } else { TopRow::Media };
        self.save_config();
        if announce && self.config.ui.osd {
            let text = if functions_first {
                tr("Верхний ряд: F1–F12", "Top row: F1–F12")
            } else {
                tr("Верхний ряд: медиаклавиши", "Top row: media keys")
            };
            osd::show(glyph::KEYBOARD, text, None);
        }
    }

    fn set_paused(&mut self, paused: bool) {
        shared().paused = paused;
        engine_cmd(CMD_SET_PAUSED, paused as isize);
        if self.config.ui.osd {
            let (g, text) = if paused {
                (glyph::PAUSE, tr("AluKeys на паузе", "AluKeys paused"))
            } else {
                (glyph::PLAY, tr("AluKeys работает", "AluKeys resumed"))
            };
            osd::show(g, text, None);
        }
        self.refresh_tray();
    }

    fn check_config_file(&mut self) {
        let m = settings::modified();
        if m == self.config_seen {
            return;
        }
        self.config_seen = m;
        let (config, err) = settings::load();
        if let Some(e) = err {
            tray::notify(self.hwnd, tr("Ошибка в файле настроек", "Settings file error"), &e, true);
            return;
        }
        if config != self.config {
            log!("ui: config reloaded");
            self.config = config;
            self.push_keymap();
            let ff = self.config.top_row.mode == TopRow::Function;
            engine_cmd(CMD_SET_FUNCTIONS_FIRST, ff as isize);
            self.refresh_tray();
        }
    }

    fn show_menu(&mut self) {
        unsafe {
            let menu = CreatePopupMenu();
            let add = |m: HMENU, flags: MENU_ITEM_FLAGS, id: usize, text: &str| {
                let w = wide(text);
                AppendMenuW(m, flags, id, w.as_ptr());
            };
            let check = |on: bool| if on { MF_CHECKED } else { MF_UNCHECKED };

            let (device_line, battery, paused) = {
                let sh = shared();
                let line = if sh.device.connected {
                    sh.device.model.map(|m| m.name.to_string()).unwrap_or_else(|| sh.device.product.clone())
                } else {
                    tr("Клавиатура Apple не подключена", "Apple keyboard not connected").to_string()
                };
                (line, sh.battery, sh.paused)
            };
            add(menu, MF_STRING | MF_GRAYED, 0, &device_line);
            if let Some(b) = battery {
                add(menu, MF_STRING | MF_GRAYED, 0, &format!("{}: {b}%", tr("Батарея", "Battery")));
            }
            add(menu, MF_SEPARATOR, 0, "");

            let media = self.config.top_row.mode == TopRow::Media;
            add(
                menu,
                MF_STRING | check(media),
                ID_TOP_MEDIA,
                tr("Верхний ряд: медиаклавиши\tfn+Esc", "Top row: media keys\tfn+Esc"),
            );
            add(
                menu,
                MF_STRING | check(!media),
                ID_TOP_FUNCTION,
                tr("Верхний ряд: F1–F12\tfn+Esc", "Top row: F1–F12\tfn+Esc"),
            );

            let presets = CreatePopupMenu();
            let p = self.config.preset;
            add(
                presets,
                MF_STRING | check(p == Preset::Mac),
                ID_PRESET_MAC,
                tr("Как на Mac (Cmd = Ctrl)", "Like a Mac (Cmd = Ctrl)"),
            );
            add(
                presets,
                MF_STRING | check(p == Preset::Pc),
                ID_PRESET_PC,
                tr("Как на ПК (Ctrl | Win | Alt)", "Like a PC (Ctrl | Win | Alt)"),
            );
            add(
                presets,
                MF_STRING | check(p == Preset::AsIs),
                ID_PRESET_ASIS,
                tr("Без изменений", "Unchanged"),
            );
            add(menu, MF_POPUP, presets as usize, tr("Модификаторы", "Modifier keys"));

            add(menu, MF_STRING | check(paused), ID_PAUSE, tr("Пауза", "Pause"));
            add(menu, MF_SEPARATOR, 0, "");
            add(menu, MF_STRING | MF_DEFAULT, ID_SETTINGS, tr("Настройки…", "Settings…"));
            add(menu, MF_STRING, ID_EDIT_CONFIG, tr("Файл настроек", "Settings file"));
            add(menu, MF_STRING, ID_OPEN_FOLDER, tr("Открыть папку программы", "Open data folder"));

            let auto = CreatePopupMenu();
            let a = self.autostart;
            add(auto, MF_STRING | check(a == Mode::Off), ID_AUTO_OFF, tr("Не запускать", "Off"));
            add(auto, MF_STRING | check(a == Mode::Normal), ID_AUTO_NORMAL, tr("Обычный запуск", "Normal"));
            add(
                auto,
                MF_STRING | check(a == Mode::Elevated),
                ID_AUTO_ELEVATED,
                tr(
                    "С правами администратора (работает и в окнах администратора)",
                    "As administrator (also works in admin windows)",
                ),
            );
            add(menu, MF_POPUP, auto as usize, tr("Запуск вместе с Windows", "Start with Windows"));
            if !self.elevated {
                add(
                    menu,
                    MF_STRING,
                    ID_ELEVATE,
                    tr("Перезапустить с правами администратора", "Restart as administrator"),
                );
            }
            add(menu, MF_SEPARATOR, 0, "");
            let logging = crate::shared::LOG_KEYS.load(Ordering::Relaxed);
            add(
                menu,
                MF_STRING | check(logging),
                ID_LOG_KEYS,
                tr("Записывать нажатия в журнал (отладка)", "Log key events (troubleshooting)"),
            );
            add(menu, MF_STRING, ID_ABOUT, tr("О программе", "About"));
            add(menu, MF_STRING, ID_EXIT, tr("Выход", "Exit"));

            let mut pt = POINT { x: 0, y: 0 };
            GetCursorPos(&mut pt);
            SetForegroundWindow(self.hwnd);
            let id =
                TrackPopupMenuEx(menu, TPM_RETURNCMD | TPM_RIGHTBUTTON, pt.x, pt.y, self.hwnd, null_mut());
            DestroyMenu(menu);
            // Required after TrackPopupMenu so the menu closes properly next time.
            PostMessageW(self.hwnd, WM_NULL, 0, 0);
            if id != 0 {
                self.on_command(id as usize);
            }
        }
    }

    fn on_command(&mut self, id: usize) {
        match id {
            ID_TOP_MEDIA | ID_TOP_FUNCTION => {
                let ff = id == ID_TOP_FUNCTION;
                engine_cmd(CMD_SET_FUNCTIONS_FIRST, ff as isize);
                self.set_fn_lock(ff, false);
            }
            ID_PRESET_MAC | ID_PRESET_PC | ID_PRESET_ASIS => {
                self.config.preset = match id {
                    ID_PRESET_MAC => Preset::Mac,
                    ID_PRESET_PC => Preset::Pc,
                    _ => Preset::AsIs,
                };
                self.save_config();
                self.push_keymap();
            }
            ID_PAUSE => {
                let p = !shared().paused;
                self.set_paused(p);
                self.send_state();
            }
            ID_SETTINGS => unsafe {
                PostMessageW(self.hwnd, WM_OPEN_SETTINGS, 0, 0);
            },
            ID_EDIT_CONFIG => open("notepad.exe", &format!("\"{}\"", settings::config_path().display())),
            ID_OPEN_FOLDER => open(&settings::data_dir().display().to_string(), ""),
            ID_AUTO_OFF | ID_AUTO_NORMAL | ID_AUTO_ELEVATED => {
                let mode = match id {
                    ID_AUTO_OFF => Mode::Off,
                    ID_AUTO_NORMAL => Mode::Normal,
                    _ => Mode::Elevated,
                };
                if !autostart::set(mode) {
                    tray::notify(
                        self.hwnd,
                        "AluKeys",
                        tr("Не удалось изменить автозапуск.", "Could not change the startup setting."),
                        true,
                    );
                }
                self.autostart = autostart::current();
                self.config.ui.start_with_windows = self.autostart != Mode::Off;
                self.save_config();
                self.send_state();
            }
            ID_ELEVATE => {
                if autostart::relaunch_elevated() {
                    // The elevated copy (--replace) closes us.
                    log!("ui: relaunching elevated");
                }
            }
            ID_ABOUT => {
                let text = format!(
                    "AluKeys {}\n\n{}\n\n{}",
                    env!("CARGO_PKG_VERSION"),
                    tr(
                        "Поддержка клавиатур Apple в Windows: fn, Eject, медиаклавиши, сочетания как на Mac.\nСвободная программа (MIT).",
                        "Apple keyboard support for Windows: fn, Eject, media keys, Mac-style shortcuts.\nFree software (MIT)."
                    ),
                    settings::config_path().display()
                );
                let t = wide(&text);
                let c = wide("AluKeys");
                let params = MSGBOXPARAMSW {
                    cbSize: std::mem::size_of::<MSGBOXPARAMSW>() as u32,
                    hwndOwner: self.hwnd,
                    hInstance: unsafe { GetModuleHandleW(null_mut()) },
                    lpszText: t.as_ptr(),
                    lpszCaption: c.as_ptr(),
                    dwStyle: MB_OK | MB_USERICON,
                    lpszIcon: 1 as _,
                    ..unsafe { zeroed() }
                };
                unsafe { MessageBoxIndirectW(&params) };
            }
            ID_LOG_KEYS => {
                let on = !crate::shared::LOG_KEYS.load(Ordering::Relaxed);
                crate::shared::LOG_KEYS.store(on, Ordering::Relaxed);
                log!("key log {}", if on { "on" } else { "off" });
            }
            ID_EXIT => quit(self.hwnd),
            _ => {}
        }
    }
}

fn open(file: &str, params: &str) {
    let f = wide(file);
    let p = wide(params);
    let verb = wide("open");
    unsafe { ShellExecuteW(null_mut(), verb.as_ptr(), f.as_ptr(), p.as_ptr(), null_mut(), SW_SHOWNORMAL) };
}

fn quit(hwnd: HWND) {
    settings_window::close();
    engine_cmd(CMD_QUIT, 0);
    tray::remove(hwnd);
    unsafe { PostQuitMessage(0) };
}

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_TRAY => {
            let event = (lparam & 0xFFFF) as u32;
            // Version 4 icons report a left click as NIN_SELECT and a right click as WM_CONTEXTMENU.
            if event == WM_CONTEXTMENU {
                with(|a| a.show_menu());
            } else if event == NIN_SELECT || event == NIN_KEYSELECT {
                unsafe { PostMessageW(hwnd, WM_OPEN_SETTINGS, 0, 0) };
            }
            0
        }
        WM_SHOW_MENU => {
            unsafe { PostMessageW(hwnd, WM_OPEN_SETTINGS, 0, 0) };
            0
        }
        WM_OPEN_SETTINGS => {
            // Create outside of `with`: WebView2 start-up pumps messages re-entrantly.
            crate::shared::TRACE_ON.store(true, Ordering::Relaxed);
            settings_window::open(if wparam == 1 { "wizard" } else { "keyboard" }, crate::util::apps_dark());
            if !settings_window::is_open() {
                crate::shared::TRACE_ON.store(false, Ordering::Relaxed);
            }
            0
        }
        WM_SETTINGS_IPC => {
            for m in settings_window::take_messages() {
                with(|a| a.on_ipc(&m));
            }
            0
        }
        WM_SETTINGS_CLOSED => {
            crate::shared::TRACE_ON.store(false, Ordering::Relaxed);
            if let Ok(mut q) = crate::shared::TRACE.lock() {
                q.clear();
            }
            0
        }
        WM_TRACE => {
            let events: Vec<_> =
                crate::shared::TRACE.lock().map(|mut q| q.drain(..).collect()).unwrap_or_default();
            if crate::shared::LOG_KEYS.load(Ordering::Relaxed) {
                for e in &events {
                    log!("{}", ui_state::trace_line(e));
                }
            }
            if !events.is_empty() && settings_window::is_open() {
                settings_window::send(&ui_state::trace_json(&events));
            }
            0
        }
        WM_ENGINE_EFFECT => {
            with(|a| match wparam {
                FX_BRIGHTNESS => worker::submit(Job::Brightness(lparam as i8)),
                FX_FN_LOCK => a.set_fn_lock(lparam != 0, true),
                FX_LOCK => unsafe {
                    LockWorkStation();
                },
                _ => {}
            });
            0
        }
        WM_BRIGHTNESS_DONE => {
            with(|a| {
                if !a.config.ui.osd {
                    return;
                }
                if lparam >= 0 {
                    osd::show(glyph::BRIGHTNESS, tr("Яркость", "Brightness"), Some(lparam as u32));
                } else if !a.ddc_warned {
                    a.ddc_warned = true;
                    osd::show(
                        glyph::WARNING,
                        tr("Монитор не поддерживает DDC/CI", "Monitor does not support DDC/CI"),
                        None,
                    );
                }
            });
            0
        }
        WM_BATTERY => {
            with(|a| a.on_battery(lparam.clamp(0, 100) as u8));
            0
        }
        WM_DEVICE_CHANGED => {
            with(|a| a.on_device_changed(false));
            0
        }
        WM_TIMER => {
            match wparam {
                TIMER_CONFIG => {
                    with(|a| a.check_config_file());
                }
                TIMER_BATTERY_SOON => {
                    unsafe { KillTimer(hwnd, TIMER_BATTERY_SOON) };
                    worker::submit(Job::Battery);
                }
                TIMER_BATTERY if shared().device.connected => worker::submit(Job::Battery),
                _ => {}
            }
            0
        }
        WM_WTSSESSION_CHANGE => {
            // Keys held across a lock or session switch never see their release.
            if matches!(wparam as u32, WTS_SESSION_LOCK | WTS_SESSION_UNLOCK) {
                engine_cmd(CMD_RESET, 0);
            }
            0
        }
        WM_POWERBROADCAST => {
            if matches!(wparam as u32, PBT_APMSUSPEND | PBT_APMRESUMEAUTOMATIC) {
                engine_cmd(CMD_RESET, 0);
            }
            1
        }
        WM_SETTINGCHANGE | WM_DPICHANGED => {
            // Taskbar theme or DPI changed: redraw the icon.
            with(|a| a.refresh_tray());
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
        WM_CLOSE | WM_ENDSESSION => {
            quit(hwnd);
            0
        }
        _ => {
            let taskbar = with(|a| a.taskbar_created).unwrap_or(0);
            if taskbar != 0 && msg == taskbar {
                // Explorer restarted: put the icon back.
                with(|a| {
                    tray::add(a.hwnd, a.icon, &a.tooltip());
                });
                return 0;
            }
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
    }
}

const WTS_SESSION_LOCK: u32 = 7;
const WTS_SESSION_UNLOCK: u32 = 8;
const WM_WTSSESSION_CHANGE: u32 = 0x02B1;
const NIN_SELECT: u32 = WM_USER;
const NIN_KEYSELECT: u32 = WM_USER + 1;
