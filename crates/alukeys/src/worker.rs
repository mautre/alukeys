//! Slow jobs off the hook and UI threads: monitor brightness over DDC/CI (tens of
//! milliseconds per call) and battery feature reports.

use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use windows_sys::Win32::Devices::Display::{
    DestroyPhysicalMonitors, GetMonitorBrightness, GetNumberOfPhysicalMonitorsFromHMONITOR,
    GetPhysicalMonitorsFromHMONITOR, PHYSICAL_MONITOR, SetMonitorBrightness,
};
use windows_sys::Win32::Foundation::{LPARAM, RECT};
use windows_sys::Win32::Graphics::Gdi::{EnumDisplayMonitors, HDC, HMONITOR};
use windows_sys::Win32::UI::WindowsAndMessaging::PostMessageW;
use windows_sys::core::BOOL;

use crate::shared::{WM_BATTERY, WM_BRIGHTNESS_DONE, shared};

pub enum Job {
    Brightness(i8),
    Battery,
    /// Read the product name of an unknown model (Bluetooth round trip).
    Product,
}

static TX: OnceLock<Mutex<Sender<Job>>> = OnceLock::new();

pub fn start() {
    let (tx, rx) = channel();
    let _ = TX.set(Mutex::new(tx));
    std::thread::Builder::new().name("worker".into()).spawn(move || run(rx)).expect("worker");
}

pub fn submit(job: Job) {
    if let Some(tx) = TX.get() {
        let _ = tx.lock().unwrap().send(job);
    }
}

fn post(msg: u32, l: isize) {
    let ui = shared().ui;
    unsafe { PostMessageW(ui.get(), msg, 0, l) };
}

/// Brightness steps, like macOS.
const STEPS: u32 = 16;

struct Cache {
    /// (min, current, max) per physical monitor, read at `at`.
    values: Vec<(u32, u32, u32)>,
    at: Instant,
}

fn run(rx: Receiver<Job>) {
    let mut cache: Option<Cache> = None;
    while let Ok(job) = rx.recv() {
        match job {
            Job::Brightness(d) => {
                // Holding the key queues many steps; apply them together.
                let mut delta = d as i32;
                while let Ok(Job::Brightness(more)) = rx.try_recv() {
                    delta += more as i32;
                }
                let level = brightness(delta, &mut cache);
                post(WM_BRIGHTNESS_DONE, level.map(|l| l as isize).unwrap_or(-1));
            }
            Job::Product => {
                let paths = shared().device.paths.clone();
                if let Some(name) = paths.iter().find_map(|p| crate::devices::product_string(p)) {
                    shared().device.product = name;
                }
            }
            Job::Battery => {
                let paths = shared().device.paths.clone();
                if let Some(level) = crate::devices::read_battery(&paths) {
                    post(WM_BATTERY, level as isize);
                }
            }
        }
    }
}

unsafe extern "system" fn collect(m: HMONITOR, _: HDC, _: *mut RECT, data: LPARAM) -> BOOL {
    let v = unsafe { &mut *(data as *mut Vec<HMONITOR>) };
    v.push(m);
    1
}

/// Runs `f` on every physical monitor reachable over DDC/CI.
fn for_each_monitor(mut f: impl FnMut(usize, PHYSICAL_MONITOR)) {
    let mut handles: Vec<HMONITOR> = Vec::new();
    unsafe {
        EnumDisplayMonitors(
            std::ptr::null_mut(),
            std::ptr::null(),
            Some(collect),
            &mut handles as *mut _ as LPARAM,
        )
    };
    let mut index = 0;
    for h in handles {
        let mut n = 0u32;
        if unsafe { GetNumberOfPhysicalMonitorsFromHMONITOR(h, &mut n) } == 0 || n == 0 {
            continue;
        }
        let mut phys = vec![unsafe { std::mem::zeroed::<PHYSICAL_MONITOR>() }; n as usize];
        if unsafe { GetPhysicalMonitorsFromHMONITOR(h, n, phys.as_mut_ptr()) } == 0 {
            continue;
        }
        for p in &phys {
            f(index, *p);
            index += 1;
        }
        unsafe { DestroyPhysicalMonitors(n, phys.as_ptr()) };
    }
}

/// Changes brightness on all monitors; returns the new level (percent) of the first.
fn brightness(delta: i32, cache: &mut Option<Cache>) -> Option<u32> {
    let fresh = cache.as_ref().is_some_and(|c| c.at.elapsed() < Duration::from_secs(5));
    if !fresh {
        let mut values = Vec::new();
        for_each_monitor(|_, p| {
            let (mut min, mut cur, mut max) = (0u32, 0u32, 0u32);
            let ok = unsafe { GetMonitorBrightness(p.hPhysicalMonitor, &mut min, &mut cur, &mut max) };
            values.push(if ok != 0 && max > min { (min, cur, max) } else { (0, 0, 0) });
        });
        *cache = Some(Cache { values, at: Instant::now() });
    }
    let c = cache.as_mut()?;
    let mut first = None;
    for_each_monitor(|i, p| {
        let Some(v) = c.values.get_mut(i) else { return };
        let (min, cur, max) = *v;
        if max <= min {
            return;
        }
        let step = ((max - min) as f32 / STEPS as f32).max(1.0);
        // Snap to the step grid so levels match what the OSD shows.
        let pos = ((cur - min) as f32 / step).round() as i32 + delta;
        let new = (min as f32 + pos.clamp(0, STEPS as i32) as f32 * step).round() as u32;
        let new = new.clamp(min, max);
        if unsafe { SetMonitorBrightness(p.hPhysicalMonitor, new) } != 0 {
            v.1 = new;
        }
        if first.is_none() {
            first = Some((new - min) * 100 / (max - min));
        }
    });
    c.at = Instant::now();
    first
}
