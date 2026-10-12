//! alukeys-diag: one-shot diagnostic for Apple keyboards on Windows.
//!
//! Produces `alukeys-diag-report.txt` next to the executable with:
//! 1. system info and potentially conflicting software;
//! 2. every raw-input device, with full HID capabilities for Apple (VID 0x05AC) collections;
//! 3. feature-report probes (battery);
//! 4. a live session that logs low-level hook events and WM_INPUT (keyboard + Apple HID)
//!    with microsecond timestamps, so we can see fn/Eject reports and event ordering.

#[cfg(windows)]
mod enumerate;
#[cfg(windows)]
mod live;
#[cfg(windows)]
mod report;
#[cfg(windows)]
mod sysinfo;
#[cfg(windows)]
mod util;

#[cfg(windows)]
fn main() {
    use windows_sys::Win32::System::Console::SetConsoleOutputCP;
    unsafe { SetConsoleOutputCP(65001) };

    let path = report::init();
    out!("AluKeys diag {} ({})", env!("CARGO_PKG_VERSION"), env!("ALUKEYS_REV"));
    println!("Отчёт пишется в: {}\n", path.display());

    report::section("SYSTEM");
    sysinfo::dump();

    report::section("DEVICES");
    let apple = enumerate::dump();

    report::section("LIVE SESSION");
    live::run(&apple);

    report::section("END");
    println!("\nГотово. Пришлите файл отчёта: {}", path.display());
    println!("Нажмите Enter, чтобы закрыть окно...");
    let _ = std::io::stdin().read_line(&mut String::new());
}

#[cfg(not(windows))]
fn main() {
    eprintln!("alukeys-diag runs on Windows only.");
    std::process::exit(1);
}
