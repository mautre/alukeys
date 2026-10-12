use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

static FILE: Mutex<Option<File>> = Mutex::new(None);

/// Creates the report file next to the executable (falls back to the current directory).
pub fn init() -> PathBuf {
    let dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."));
    let mut path = dir.join("alukeys-diag-report.txt");
    let file = File::create(&path).or_else(|_| {
        path = PathBuf::from("alukeys-diag-report.txt");
        File::create(&path)
    });
    *FILE.lock().unwrap() = file.ok();
    path
}

/// Writes a line to the report file only.
pub fn file(line: &str) {
    if let Some(f) = FILE.lock().unwrap().as_mut() {
        let _ = writeln!(f, "{line}");
        let _ = f.flush();
    }
}

/// Writes a line to both the console and the report file.
pub fn both(line: &str) {
    println!("{line}");
    file(line);
}

pub fn section(name: &str) {
    both("");
    both(&format!("==================== {name} ===================="));
}

#[macro_export]
macro_rules! out {
    ($($arg:tt)*) => { $crate::report::both(&format!($($arg)*)) };
}

#[macro_export]
macro_rules! rep {
    ($($arg:tt)*) => { $crate::report::file(&format!($($arg)*)) };
}
