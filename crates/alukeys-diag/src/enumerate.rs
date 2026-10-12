use std::ffi::c_void;
use std::mem::{size_of, zeroed};
use std::ptr::null_mut;

use windows_sys::Win32::Devices::HumanInterfaceDevice::{
    HIDD_ATTRIBUTES, HIDP_BUTTON_CAPS, HIDP_CAPS, HIDP_REPORT_TYPE, HIDP_STATUS_SUCCESS, HIDP_VALUE_CAPS,
    HidD_GetAttributes, HidD_GetFeature, HidD_GetInputReport, HidD_GetManufacturerString,
    HidD_GetProductString, HidP_Feature, HidP_GetButtonCaps, HidP_GetCaps, HidP_GetUsageValue,
    HidP_GetValueCaps, HidP_Input, HidP_Output,
};
use windows_sys::Win32::Foundation::{
    CloseHandle, GENERIC_READ, GENERIC_WRITE, GetLastError, HANDLE, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows_sys::Win32::UI::Input::{
    GetRawInputDeviceInfoW, GetRawInputDeviceList, RAWINPUTDEVICELIST, RID_DEVICE_INFO, RIDI_DEVICEINFO,
    RIDI_DEVICENAME, RIDI_PREPARSEDDATA, RIM_TYPEHID, RIM_TYPEKEYBOARD, RIM_TYPEMOUSE,
};

use crate::util::{from_wide, hex, vid_pid_from_path, wide};
use crate::{out, rep};

pub const APPLE_VID: u16 = 0x05AC;

/// A HID top-level collection that belongs to an Apple device and is not a plain
/// keyboard/mouse (those are owned by Windows and cannot be registered as HID).
#[derive(Clone, Debug)]
pub struct AppleCollection {
    pub usage_page: u16,
    pub usage: u16,
}

pub fn usage_page_name(p: u16) -> &'static str {
    match p {
        0x01 => "Generic Desktop",
        0x06 => "Generic Device Controls",
        0x07 => "Keyboard/Keypad",
        0x08 => "LED",
        0x09 => "Button",
        0x0C => "Consumer",
        0x0D => "Digitizer",
        0x0F => "Physical Input Device",
        0x84 => "Power Device",
        0x85 => "Battery System",
        0x00FF => "Apple vendor (0x00FF)",
        0xFF00..=0xFFFF => "Vendor-defined",
        _ => "?",
    }
}

pub fn device_name(h: HANDLE) -> String {
    unsafe {
        let mut len = 0u32;
        GetRawInputDeviceInfoW(h, RIDI_DEVICENAME, null_mut(), &mut len);
        let mut buf = vec![0u16; len as usize + 1];
        GetRawInputDeviceInfoW(h, RIDI_DEVICENAME, buf.as_mut_ptr().cast(), &mut len);
        from_wide(&buf)
    }
}

pub fn device_info(h: HANDLE) -> Option<RID_DEVICE_INFO> {
    unsafe {
        let mut info: RID_DEVICE_INFO = zeroed();
        info.cbSize = size_of::<RID_DEVICE_INFO>() as u32;
        let mut len = info.cbSize;
        let rc =
            GetRawInputDeviceInfoW(h, RIDI_DEVICEINFO, (&mut info as *mut RID_DEVICE_INFO).cast(), &mut len);
        (rc != u32::MAX && rc != 0).then_some(info)
    }
}

fn preparsed(h: HANDLE) -> Option<Vec<u8>> {
    unsafe {
        let mut len = 0u32;
        GetRawInputDeviceInfoW(h, RIDI_PREPARSEDDATA, null_mut(), &mut len);
        if len == 0 {
            return None;
        }
        // Over-aligned buffer: preparsed data is an opaque struct read by hid.dll.
        let mut buf = vec![0u8; len as usize + 16];
        let rc = GetRawInputDeviceInfoW(h, RIDI_PREPARSEDDATA, buf.as_mut_ptr().cast(), &mut len);
        (rc != u32::MAX).then_some(buf)
    }
}

pub fn dump() -> Vec<AppleCollection> {
    let mut apple = Vec::new();
    let devices = unsafe {
        let mut n = 0u32;
        GetRawInputDeviceList(null_mut(), &mut n, size_of::<RAWINPUTDEVICELIST>() as u32);
        let mut list = vec![zeroed::<RAWINPUTDEVICELIST>(); n as usize];
        let got = GetRawInputDeviceList(list.as_mut_ptr(), &mut n, size_of::<RAWINPUTDEVICELIST>() as u32);
        list.truncate(if got == u32::MAX { 0 } else { got as usize });
        list
    };

    out!("Raw input devices: {}", devices.len());
    for d in &devices {
        let name = device_name(d.hDevice);
        let vp = vid_pid_from_path(&name);
        let is_apple = vp.map(|(v, _)| v == APPLE_VID).unwrap_or(false);
        let Some(info) = device_info(d.hDevice) else {
            continue;
        };
        let kind = match d.dwType {
            RIM_TYPEKEYBOARD => "KEYBOARD",
            RIM_TYPEMOUSE => "MOUSE",
            RIM_TYPEHID => "HID",
            _ => "?",
        };
        let tag = if is_apple { "  <-- APPLE" } else { "" };
        match d.dwType {
            RIM_TYPEHID => {
                let hid = unsafe { info.Anonymous.hid };
                out!(
                    "[{kind}] VID={:04X} PID={:04X} ver={:04X} UsagePage=0x{:04X} ({}) Usage=0x{:04X}{tag}",
                    hid.dwVendorId,
                    hid.dwProductId,
                    hid.dwVersionNumber,
                    hid.usUsagePage,
                    usage_page_name(hid.usUsagePage),
                    hid.usUsage
                );
                if hid.dwVendorId as u16 == APPLE_VID {
                    apple.push(AppleCollection { usage_page: hid.usUsagePage, usage: hid.usUsage });
                }
            }
            RIM_TYPEKEYBOARD => {
                let k = unsafe { info.Anonymous.keyboard };
                let (v, p) = vp.unwrap_or((0, 0));
                out!(
                    "[{kind}] VID={v:04X} PID={p:04X} type={} subtype={} fkeys={} leds={} keys={}{tag}",
                    k.dwType,
                    k.dwSubType,
                    k.dwNumberOfFunctionKeys,
                    k.dwNumberOfIndicators,
                    k.dwNumberOfKeysTotal
                );
            }
            _ => {
                let (v, p) = vp.unwrap_or((0, 0));
                out!("[{kind}] VID={v:04X} PID={p:04X}{tag}");
            }
        }
        rep!("    path: {name}");
        if is_apple {
            dump_hid_details(d.hDevice, &name);
        }
    }
    if apple.is_empty() {
        out!("!! Apple HID collections not found. Is the keyboard paired and connected?");
    }
    apple.sort_by_key(|c| (c.usage_page, c.usage));
    apple.dedup_by_key(|c| (c.usage_page, c.usage));
    apple
}

fn dump_hid_details(h: HANDLE, path: &str) {
    let Some(pp) = preparsed(h) else {
        rep!("    (no preparsed data)");
        return;
    };
    let ppd = pp.as_ptr() as isize;
    let mut caps: HIDP_CAPS = unsafe { zeroed() };
    if unsafe { HidP_GetCaps(ppd, &mut caps) } != HIDP_STATUS_SUCCESS {
        rep!("    HidP_GetCaps failed");
        return;
    }
    rep!(
        "    caps: in={}B out={}B feat={}B  inBtn={} inVal={} featBtn={} featVal={} links={}",
        caps.InputReportByteLength,
        caps.OutputReportByteLength,
        caps.FeatureReportByteLength,
        caps.NumberInputButtonCaps,
        caps.NumberInputValueCaps,
        caps.NumberFeatureButtonCaps,
        caps.NumberFeatureValueCaps,
        caps.NumberLinkCollectionNodes
    );

    let mut feature_ids = Vec::new();
    let mut input_ids = Vec::new();
    for (ty, label) in [(HidP_Input, "IN"), (HidP_Output, "OUT"), (HidP_Feature, "FEAT")] {
        for b in button_caps(ty, ppd) {
            let (lo, hi) = unsafe {
                if b.IsRange {
                    (b.Anonymous.Range.UsageMin, b.Anonymous.Range.UsageMax)
                } else {
                    (b.Anonymous.NotRange.Usage, b.Anonymous.NotRange.Usage)
                }
            };
            rep!(
                "    {label:4} btn  rid=0x{:02X} page=0x{:04X} ({}) usage=0x{lo:04X}..0x{hi:04X} link=0x{:04X}/0x{:04X}",
                b.ReportID,
                b.UsagePage,
                usage_page_name(b.UsagePage),
                b.LinkUsagePage,
                b.LinkUsage
            );
            collect_id(ty, b.ReportID, &mut input_ids, &mut feature_ids);
        }
        for v in value_caps(ty, ppd) {
            let (lo, hi) = unsafe {
                if v.IsRange {
                    (v.Anonymous.Range.UsageMin, v.Anonymous.Range.UsageMax)
                } else {
                    (v.Anonymous.NotRange.Usage, v.Anonymous.NotRange.Usage)
                }
            };
            let battery = if v.UsagePage == 0x06 && lo == 0x20 { "  <-- BATTERY STRENGTH" } else { "" };
            rep!(
                "    {label:4} val  rid=0x{:02X} page=0x{:04X} ({}) usage=0x{lo:04X}..0x{hi:04X} bits={}x{} logical={}..{}{battery}",
                v.ReportID,
                v.UsagePage,
                usage_page_name(v.UsagePage),
                v.BitSize,
                v.ReportCount,
                v.LogicalMin,
                v.LogicalMax
            );
            collect_id(ty, v.ReportID, &mut input_ids, &mut feature_ids);
        }
    }

    // Open the device: try read/write first (needed by some feature reports), then query-only.
    let handle = open(path, true).or_else(|| open(path, false));
    let Some((handle, rw)) = handle else {
        rep!("    CreateFile failed: error {}", unsafe { GetLastError() });
        return;
    };
    rep!("    opened with {} access", if rw { "read/write" } else { "query-only" });
    unsafe {
        let mut attr: HIDD_ATTRIBUTES = zeroed();
        attr.Size = size_of::<HIDD_ATTRIBUTES>() as u32;
        if HidD_GetAttributes(handle, &mut attr) {
            rep!(
                "    attributes: VID={:04X} PID={:04X} ver={:04X}",
                attr.VendorID,
                attr.ProductID,
                attr.VersionNumber
            );
        }
        let mut s = [0u16; 128];
        if HidD_GetManufacturerString(handle, s.as_mut_ptr().cast(), 256) {
            rep!("    manufacturer: {}", from_wide(&s));
        }
        let mut s = [0u16; 128];
        if HidD_GetProductString(handle, s.as_mut_ptr().cast(), 256) {
            out!("    product: {}", from_wide(&s));
        }
    }

    for &id in &feature_ids {
        probe(handle, ppd, HidP_Feature, id, caps.FeatureReportByteLength);
    }
    for &id in &input_ids {
        probe(handle, ppd, HidP_Input, id, caps.InputReportByteLength);
    }
    unsafe { CloseHandle(handle) };
}

fn collect_id(ty: HIDP_REPORT_TYPE, id: u8, input: &mut Vec<u8>, feature: &mut Vec<u8>) {
    let v = if ty == HidP_Feature {
        feature
    } else if ty == HidP_Input {
        input
    } else {
        return;
    };
    if !v.contains(&id) {
        v.push(id);
    }
}

fn button_caps(ty: HIDP_REPORT_TYPE, ppd: isize) -> Vec<HIDP_BUTTON_CAPS> {
    let mut n: u16 = 64;
    let mut v = vec![unsafe { zeroed::<HIDP_BUTTON_CAPS>() }; n as usize];
    if unsafe { HidP_GetButtonCaps(ty, v.as_mut_ptr(), &mut n, ppd) } != HIDP_STATUS_SUCCESS {
        return Vec::new();
    }
    v.truncate(n as usize);
    v
}

fn value_caps(ty: HIDP_REPORT_TYPE, ppd: isize) -> Vec<HIDP_VALUE_CAPS> {
    let mut n: u16 = 64;
    let mut v = vec![unsafe { zeroed::<HIDP_VALUE_CAPS>() }; n as usize];
    if unsafe { HidP_GetValueCaps(ty, v.as_mut_ptr(), &mut n, ppd) } != HIDP_STATUS_SUCCESS {
        return Vec::new();
    }
    v.truncate(n as usize);
    v
}

fn open(path: &str, rw: bool) -> Option<(HANDLE, bool)> {
    let p = wide(path);
    let access = if rw { GENERIC_READ | GENERIC_WRITE } else { 0 };
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
    (h != INVALID_HANDLE_VALUE).then_some((h, rw))
}

/// Requests a report by ID and prints raw bytes plus a decoded battery value if present.
fn probe(handle: HANDLE, ppd: isize, ty: HIDP_REPORT_TYPE, id: u8, len: u16) {
    if len == 0 {
        return;
    }
    let mut buf = vec![0u8; len as usize];
    buf[0] = id;
    let (ok, label) = unsafe {
        if ty == HidP_Feature {
            (HidD_GetFeature(handle, buf.as_mut_ptr() as *mut c_void, len as u32), "GetFeature")
        } else {
            (HidD_GetInputReport(handle, buf.as_mut_ptr() as *mut c_void, len as u32), "GetInputReport")
        }
    };
    if !ok {
        rep!("    {label}(0x{id:02X}) failed: error {}", unsafe { GetLastError() });
        return;
    }
    let mut line = format!("    {label}(0x{id:02X}) = {}", hex(&buf));
    let mut value = 0u32;
    let st = unsafe { HidP_GetUsageValue(ty, 0x06, 0, 0x20, &mut value, ppd, buf.as_mut_ptr(), len as u32) };
    if st == HIDP_STATUS_SUCCESS {
        line.push_str(&format!("  => BATTERY {value}"));
        out!("    Battery strength (report 0x{id:02X}): {value}");
    }
    rep!("{line}");
}

/// Tries every way to read battery report 0x47 (input/feature, several lengths) on every
/// Apple collection, logging each attempt. Apple declares it as an input report but may
/// only answer it as a feature report.
pub fn battery_probe() {
    const ID: u8 = 0x47;
    let paths: Vec<String> = unsafe {
        let mut n = 0u32;
        GetRawInputDeviceList(null_mut(), &mut n, size_of::<RAWINPUTDEVICELIST>() as u32);
        let mut list = vec![zeroed::<RAWINPUTDEVICELIST>(); n as usize];
        let got = GetRawInputDeviceList(list.as_mut_ptr(), &mut n, size_of::<RAWINPUTDEVICELIST>() as u32);
        list.truncate(if got == u32::MAX { 0 } else { got as usize });
        list.iter().map(|d| device_name(d.hDevice)).collect()
    };
    let apple: Vec<String> =
        paths.into_iter().filter(|p| vid_pid_from_path(p).is_some_and(|(v, _)| v == APPLE_VID)).collect();
    let mut found = false;
    for path in &apple {
        let short = path.split('&').find(|s| s.starts_with("Col")).unwrap_or("?").to_string();
        for (rw, access) in [(true, "rw"), (false, "query")] {
            let Some((h, _)) = open(path, rw) else {
                rep!("  {short} [{access}]: open failed: error {}", unsafe { GetLastError() });
                continue;
            };
            for feature in [true, false] {
                for len in [2u32, 3, 4, 8, 16] {
                    let mut buf = vec![0u8; len as usize];
                    buf[0] = ID;
                    let ok = unsafe {
                        if feature {
                            HidD_GetFeature(h, buf.as_mut_ptr() as *mut c_void, len)
                        } else {
                            HidD_GetInputReport(h, buf.as_mut_ptr() as *mut c_void, len)
                        }
                    };
                    let kind = if feature { "feature" } else { "input" };
                    if ok {
                        found = true;
                        out!(
                            "  {short} [{access}] {kind} len={len}: OK {}  => battery raw {} (~{}%)",
                            hex(&buf),
                            buf[1],
                            buf[1] as u32 * 100 / 255
                        );
                    } else {
                        rep!("  {short} [{access}] {kind} len={len}: error {}", unsafe { GetLastError() });
                    }
                }
            }
            unsafe { CloseHandle(h) };
        }
    }
    if !found {
        out!(
            "  battery: no request succeeded (the keyboard may still send report 0x47 on its own; watch RAWHID 47 lines below)"
        );
    }
}
