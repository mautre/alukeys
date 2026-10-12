//! Where files live, and loading/saving the config.
//!
//! Installed: `%APPDATA%\AluKeys\config.toml`. Portable: if a file named `portable`
//! sits next to the executable, everything is kept in the executable's folder.

use std::path::PathBuf;
use std::time::SystemTime;

use alukeys_core::Config;

pub fn data_dir() -> PathBuf {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."));
    if exe_dir.join("portable").exists() {
        return exe_dir;
    }
    let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or(exe_dir);
    let dir = base.join("AluKeys");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

pub fn config_path() -> PathBuf {
    data_dir().join("config.toml")
}

pub fn log_path() -> PathBuf {
    data_dir().join("alukeys.log")
}

pub fn modified() -> Option<SystemTime> {
    std::fs::metadata(config_path()).and_then(|m| m.modified()).ok()
}

/// Loads the config, creating it with defaults on first run.
/// A broken file is kept (renamed) and replaced by defaults, with the error returned.
pub fn load() -> (Config, Option<String>) {
    let path = config_path();
    match std::fs::read_to_string(&path) {
        Ok(text) => match Config::from_toml(&text) {
            Ok(c) => (c, None),
            Err(e) => {
                let _ = std::fs::rename(&path, path.with_extension("toml.broken"));
                let c = Config::default();
                save(&c);
                (c, Some(e))
            }
        },
        Err(_) => {
            let c = Config::default();
            save(&c);
            (c, None)
        }
    }
}

pub fn save(c: &Config) {
    let header = "# AluKeys settings. Edit here or in the settings window; changes apply instantly.\n\
                  # Docs: https://github.com/mautre/alukeys#configuration\n\n";
    let text = format!("{header}{}", c.to_toml());
    let path = config_path();
    let tmp = path.with_extension("toml.tmp");
    if std::fs::write(&tmp, text).is_ok() {
        let _ = std::fs::rename(&tmp, &path);
    }
}
