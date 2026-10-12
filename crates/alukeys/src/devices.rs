//! Finding the Apple keyboard, reading its vendor report (fn, Eject) and its battery.

use std::ffi::c_void;
use std::mem::{size_of, zeroed};
use std::ptr::null_mut;

use alukeys_core::model::{self, APPLE_VID};
use windows_sys::Win32::Devices::HumanInterfaceDevice::{
    HIDP_REPORT_TYPE, HIDP_STATUS_SUCCESS, HIDP_VALUE_CAPS, HidD_GetFeature, HidD_GetInputReport,
    HidD_GetProductString, HidP_Feature, HidP_GetUsageValue, HidP_GetValueCaps, HidP_Input,
};
use windows_sys::Win32::Foundation::{
    CloseHandle, GENERIC_READ, GENERIC_WRITE, HANDLE, HWND, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows_sys::Win32::UI::Input::{
    GetRawInputDeviceInfoW, GetRawInputDeviceList, RAWINPUTDEVICE, RAWINPUTDEVICELIST, RID_DEVICE_INFO,
    RIDEV_DEVNOTIFY, RIDEV_INPUTSINK, RIDI_DEVICEINFO, RIDI_DEVICENAME, RIDI_PREPARSEDDATA, RIM_TYPEHID,
    RIM_TYPEKEYBOARD, RegisterRawInputDevices,
};

use crate::log;
use crate::shared::Device;
use crate::util::{from_wide, wide};

/// Apple vendor report carrying fn and Eject (Apple Wireless Keyboard).
pub const FN_REPORT_ID: u8 = 0x11;
pub const FN_BIT: u8 = 0x10;
pub const EJECT_BIT: u8 = 0x08;

const BATTERY_PAGE: u16 = 0x06;
const BATTERY_USAGE: u16 = 0x20;

pub fn device_name(h: HANDLE) -> String {
    unsafe {
        let mut len = 0u32;
        GetRawInputDeviceInfoW(h, RIDI_DEVICENAME, null_mut(), &mut len);
        let mut buf = vec![0u16; len as usize + 1];
        GetRawInputDeviceInfoW(h, RIDI_DEVICENAME, buf.as_mut_ptr().cast(), &mut len);
        from_wide(&buf)
    }
}

fn device_info(h: HANDLE) -> Option<RID_DEVICE_INFO> {
    unsafe {
        let mut info: RID_DEVICE_INFO = zeroed();
        info.cbSize = size_of::<RID_DEVICE_INFO>() as u32;
        let mut len = info.cbSize;
        let rc =
            GetRawInputDeviceInfoW(h, RIDI_DEVICEINFO, (&mut info as *mut RID_DEVICE_INFO).cast(), &mut len);
        (rc != u32::MAX && rc != 0).then_some(info)
    }
}

/// Preparsed HID data of a raw input device (opaque blob for hid.dll).
pub fn preparsed(h: HANDLE) -> Option<Vec<u8>> {
    unsafe {
        let mut len = 0u32;
        GetRawInputDeviceInfoW(h, RIDI_PREPARSEDDATA, null_mut(), &mut len);
        if len == 0 {
            return None;
        }
        let mut buf = vec![0u8; len as usize];
        let rc = GetRawInputDeviceInfoW(h, RIDI_PREPARSEDDATA, buf.as_mut_ptr().cast(), &mut len);
        (rc != u32::MAX).then_some(buf)
    }
}

/// Extracts VID/PID from a device path: USB `VID_05AC&PID_0256`, Bluetooth `VID&000205AC_PID&0256`.
pub fn vid_pid(path: &str) -> Option<(u16, u16)> {
    let up = path.to_ascii_uppercase();
    let vid = match (up.find("VID_"), up.find("VID&")) {
        (Some(i), _) => u16::from_str_radix(up.get(i + 4..i + 8)?, 16).ok()?,
        // Bluetooth: VID&<source><vid>, e.g. VID&000205ac (source 0002); the VID is the last 4 digits.
        (None, Some(i)) => {
            let digits: String = up[i + 4..].chars().take_while(|c| c.is_ascii_hexdigit()).collect();
            u16::from_str_radix(digits.get(digits.len().checked_sub(4)?..)?, 16).ok()?
        }
        _ => return None,
    };
    let i = up.find("PID_").or_else(|| up.find("PID&"))?;
    let pid = u16::from_str_radix(up.get(i + 4..i + 8)?, 16).ok()?;
    Some((vid, pid))
}

/// One Apple HID collection we can register for raw input.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Collection {
    pub page: u16,
    pub usage: u16,
}

/// Scans connected devices for Apple keyboards.
pub fn scan() -> (Device, Vec<Collection>) {
    let list = unsafe {
        let mut n = 0u32;
        GetRawInputDeviceList(null_mut(), &mut n, size_of::<RAWINPUTDEVICELIST>() as u32);
        let mut list = vec![zeroed::<RAWINPUTDEVICELIST>(); n as usize];
        let got = GetRawInputDeviceList(list.as_mut_ptr(), &mut n, size_of::<RAWINPUTDEVICELIST>() as u32);
        list.truncate(if got == u32::MAX { 0 } else { got as usize });
        list
    };

    let mut dev = Device::default();
    let mut collections = Vec::new();
    for d in list {
        let path = device_name(d.hDevice);
        let Some((vid, pid)) = vid_pid(&path) else {
            continue;
        };
        if vid != APPLE_VID {
            continue;
        }
        let known = model::lookup(pid);
        match d.dwType {
            RIM_TYPEKEYBOARD => {
                dev.connected = true;
                dev.pid = pid;
                dev.model = known.copied().or(dev.model);
                dev.paths.push(path);
            }
            RIM_TYPEHID => {
                if let Some(info) = device_info(d.hDevice) {
                    let hid = unsafe { info.Anonymous.hid };
                    let c = Collection { page: hid.usUsagePage, usage: hid.usUsage };
                    if !collections.contains(&c) {
                        collections.push(c);
                    }
                }
                if known.is_some() {
                    dev.connected = true;
                    dev.pid = pid;
                    dev.model = known.copied();
                }
                dev.paths.push(path);
            }
            _ => {}
        }
    }
    (dev, collections)
}

fn open(path: &str) -> Option<HANDLE> {
    let p = wide(path);
    for access in [GENERIC_READ | GENERIC_WRITE, 0] {
        let h = unsafe {
            CreateFileW(
                p.as_ptr(),
                access,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                null_mut(),
                OPEN_EXISTING,
                0,
                null_mut(),
            )
        };
        if h != INVALID_HANDLE_VALUE {
            return Some(h);
        }
    }
    None
}

pub fn product_string(path: &str) -> Option<String> {
    let h = open(path)?;
    let mut s = [0u16; 128];
    let ok = unsafe { HidD_GetProductString(h, s.as_mut_ptr().cast(), 256) };
    unsafe { CloseHandle(h) };
    let s = from_wide(&s);
    (ok && !s.is_empty()).then_some(s)
}

/// Registers raw input for keyboards (arrival/removal and the hook watchdog) and for
/// the Apple vendor collections (fn and Eject).
pub fn register(hwnd: HWND, collections: &[Collection]) {
    let mut devs = vec![RAWINPUTDEVICE {
        usUsagePage: 0x01,
        usUsage: 0x06,
        dwFlags: RIDEV_INPUTSINK | RIDEV_DEVNOTIFY,
        hwndTarget: hwnd,
    }];
    for c in collections {
        // Keyboards and mice belong to Windows and cannot be read as raw HID.
        if c.page == 0x01 && matches!(c.usage, 0x02 | 0x06) {
            continue;
        }
        devs.push(RAWINPUTDEVICE {
            usUsagePage: c.page,
            usUsage: c.usage,
            dwFlags: RIDEV_INPUTSINK | RIDEV_DEVNOTIFY,
            hwndTarget: hwnd,
        });
    }
    for d in &devs {
        let ok = unsafe { RegisterRawInputDevices(d, 1, size_of::<RAWINPUTDEVICE>() as u32) };
        log!("raw input page={:04X} usage={:04X}: {}", d.usUsagePage, d.usUsage, ok != 0);
    }
}

/// Decodes fn/Eject from an Apple vendor report.
pub fn parse_fn_report(data: &[u8]) -> Option<(bool, bool)> {
    (data.len() >= 2 && data[0] == FN_REPORT_ID).then(|| (data[1] & FN_BIT != 0, data[1] & EJECT_BIT != 0))
}

fn scale(value: u32, caps: &HIDP_VALUE_CAPS) -> u8 {
    let max = caps.LogicalMax.max(1) as u32;
    if max <= 100 { value.min(100) as u8 } else { (value * 100 / max).min(100) as u8 }
}

fn battery_caps(ty: HIDP_REPORT_TYPE, ppd: isize) -> Option<HIDP_VALUE_CAPS> {
    let mut n: u16 = 64;
    let mut v = vec![unsafe { zeroed::<HIDP_VALUE_CAPS>() }; n as usize];
    if unsafe { HidP_GetValueCaps(ty, v.as_mut_ptr(), &mut n, ppd) } != HIDP_STATUS_SUCCESS {
        return None;
    }
    v.truncate(n as usize);
    v.into_iter().find(|c| {
        let usage =
            unsafe { if c.IsRange { c.Anonymous.Range.UsageMin } else { c.Anonymous.NotRange.Usage } };
        c.UsagePage == BATTERY_PAGE && usage == BATTERY_USAGE
    })
}

/// Battery level carried in an input report, if this device reports it that way.
pub fn battery_from_input(pp: &[u8], report: &[u8]) -> Option<u8> {
    let ppd = pp.as_ptr() as isize;
    let caps = battery_caps(HidP_Input, ppd)?;
    if report.first() != Some(&caps.ReportID) {
        return None;
    }
    let mut buf = report.to_vec();
    let mut value = 0u32;
    let st = unsafe {
        HidP_GetUsageValue(
            HidP_Input,
            BATTERY_PAGE,
            0,
            BATTERY_USAGE,
            &mut value,
            ppd,
            buf.as_mut_ptr(),
            buf.len() as u32,
        )
    };
    (st == HIDP_STATUS_SUCCESS).then(|| scale(value, &caps))
}

/// Set once every request kind has failed: Windows' HID class driver rejects requests the
/// descriptor does not declare (the A1314 case), so the level only arrives when the
/// keyboard sends report 0x47 by itself, which `battery_from_input` handles.
static BATTERY_REQUESTS_FAIL: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Where the battery level was last read successfully, so later polls go straight there.
static BATTERY_ROUTE: std::sync::Mutex<Option<(String, bool, usize)>> = std::sync::Mutex::new(None);

/// Battery report location from the HID descriptors: report ID and logical maximum.
fn battery_report(paths: &[String]) -> Option<(u8, i32)> {
    use windows_sys::Win32::Devices::HumanInterfaceDevice::{HidD_FreePreparsedData, HidD_GetPreparsedData};
    for path in paths {
        let Some(h) = open(path) else { continue };
        let mut ppd: isize = 0;
        let found = unsafe { HidD_GetPreparsedData(h, &mut ppd) }.then(|| {
            let caps = battery_caps(HidP_Input, ppd).or_else(|| battery_caps(HidP_Feature, ppd));
            unsafe { HidD_FreePreparsedData(ppd) };
            caps.map(|c| (c.ReportID, c.LogicalMax))
        });
        unsafe { CloseHandle(h) };
        if let Some(Some(r)) = found {
            return Some(r);
        }
    }
    None
}

/// Requests one report; `feature` picks GET_FEATURE over GET_INPUT_REPORT.
fn request(path: &str, id: u8, feature: bool, len: usize) -> Option<Vec<u8>> {
    let h = open(path)?;
    let mut buf = vec![0u8; len];
    buf[0] = id;
    let ok = unsafe {
        if feature {
            HidD_GetFeature(h, buf.as_mut_ptr() as *mut c_void, len as u32)
        } else {
            HidD_GetInputReport(h, buf.as_mut_ptr() as *mut c_void, len as u32)
        }
    };
    unsafe { CloseHandle(h) };
    (ok && buf[0] == id).then_some(buf)
}

/// Reads the battery level (percent). Apple keyboards declare it as input report 0x47
/// but, like Linux's HID_BATTERY_QUIRK_FEATURE notes, may only answer a feature request;
/// so several request kinds and lengths are tried on every Apple collection.
/// Blocking (Bluetooth round trip): call off the hook thread.
pub fn read_battery(paths: &[String]) -> Option<u8> {
    use std::sync::atomic::Ordering;
    if BATTERY_REQUESTS_FAIL.load(Ordering::Relaxed) {
        return None;
    }
    let (id, max) = battery_report(paths)?;
    let decode = |buf: &[u8]| {
        let v = *buf.get(1)? as u32;
        let max = max.max(1) as u32;
        Some(if max <= 100 { v.min(100) } else { (v * 100 / max).min(100) } as u8)
    };
    let known = BATTERY_ROUTE.lock().ok().and_then(|r| r.clone());
    if let Some((path, feature, len)) = known
        && let Some(buf) = request(&path, id, feature, len)
    {
        return decode(&buf);
    }
    for path in paths {
        for feature in [true, false] {
            for len in [2usize, 3, 4, 8, 16] {
                if let Some(buf) = request(path, id, feature, len) {
                    crate::log!(
                        "battery: report {id:02X} via {} len {len} on {path}",
                        if feature { "feature" } else { "input" }
                    );
                    if let Ok(mut r) = BATTERY_ROUTE.lock() {
                        *r = Some((path.clone(), feature, len));
                    }
                    return decode(&buf);
                }
            }
        }
    }
    crate::log!("battery: report {id:02X} cannot be requested; waiting for the keyboard to send it");
    BATTERY_REQUESTS_FAIL.store(true, Ordering::Relaxed);
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_paths() {
        assert_eq!(
            vid_pid(r"\\?\HID#{00001124-0000-1000-8000-00805f9b34fb}_VID&0205ac_PID&0256&Col02#8&1"),
            Some((0x05AC, 0x0256))
        );
        // Path as reported by Windows 11 25H2 for an A1314 (from a diagnostic report).
        assert_eq!(
            vid_pid(
                r"\\?\HID#{00001124-0000-1000-8000-00805f9b34fb}_VID&000205ac_PID&0256&Col01#8&25243724&0&0000#{884b96c3-56ef-11d1-bc8c-00a0c91405dd}"
            ),
            Some((0x05AC, 0x0256))
        );
        assert_eq!(vid_pid(r"\\?\HID#VID_05AC&PID_0250&MI_00#7&2"), Some((0x05AC, 0x0250)));
    }

    #[test]
    fn parses_fn_report() {
        assert_eq!(parse_fn_report(&[0x11, 0x10]), Some((true, false)));
        assert_eq!(parse_fn_report(&[0x11, 0x08]), Some((false, true)));
        assert_eq!(parse_fn_report(&[0x13, 0x10]), None);
    }
}
