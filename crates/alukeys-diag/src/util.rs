use std::ptr::null_mut;

use windows_sys::Win32::Foundation::ERROR_SUCCESS;
use windows_sys::Win32::System::Registry::{
    HKEY, KEY_READ, REG_DWORD, REG_EXPAND_SZ, REG_MULTI_SZ, REG_SZ, RegCloseKey, RegEnumKeyExW,
    RegOpenKeyExW, RegQueryValueExW,
};

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn from_wide(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(" ")
}

/// Extracts VID/PID from a device interface path. Handles both USB (`VID_05AC&PID_0256`)
/// and Bluetooth (`VID&000205AC_PID&0256`) spellings.
pub fn vid_pid_from_path(path: &str) -> Option<(u16, u16)> {
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

pub struct RegKey(pub HKEY);

impl RegKey {
    pub fn open(root: HKEY, path: &str) -> Option<RegKey> {
        let mut key: HKEY = null_mut();
        let p = wide(path);
        let rc = unsafe { RegOpenKeyExW(root, p.as_ptr(), 0, KEY_READ, &mut key) };
        (rc == ERROR_SUCCESS).then_some(RegKey(key))
    }

    pub fn subkeys(&self) -> Vec<String> {
        let mut out = Vec::new();
        for i in 0.. {
            let mut name = [0u16; 256];
            let mut len = name.len() as u32;
            let rc = unsafe {
                RegEnumKeyExW(
                    self.0,
                    i,
                    name.as_mut_ptr(),
                    &mut len,
                    null_mut(),
                    null_mut(),
                    null_mut(),
                    null_mut(),
                )
            };
            if rc != ERROR_SUCCESS {
                break;
            }
            out.push(from_wide(&name[..len as usize]));
        }
        out
    }

    /// Reads a value as a display string (REG_SZ, REG_EXPAND_SZ, REG_MULTI_SZ, REG_DWORD).
    pub fn value(&self, name: &str) -> Option<String> {
        let n = wide(name);
        let mut ty = 0u32;
        let mut size = 0u32;
        let rc = unsafe { RegQueryValueExW(self.0, n.as_ptr(), null_mut(), &mut ty, null_mut(), &mut size) };
        if rc != ERROR_SUCCESS {
            return None;
        }
        let mut buf = vec![0u8; size as usize + 4];
        let rc =
            unsafe { RegQueryValueExW(self.0, n.as_ptr(), null_mut(), &mut ty, buf.as_mut_ptr(), &mut size) };
        if rc != ERROR_SUCCESS {
            return None;
        }
        buf.truncate(size as usize);
        match ty {
            REG_DWORD if buf.len() >= 4 => {
                Some(format!("0x{:X}", u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]])))
            }
            REG_SZ | REG_EXPAND_SZ | REG_MULTI_SZ => {
                let w: Vec<u16> = buf.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).collect();
                let s: Vec<String> =
                    w.split(|&c| c == 0).filter(|p| !p.is_empty()).map(String::from_utf16_lossy).collect();
                Some(s.join(" | "))
            }
            _ => Some(format!("<type {ty}> {}", hex(&buf))),
        }
    }
}

impl Drop for RegKey {
    fn drop(&mut self) {
        unsafe { RegCloseKey(self.0) };
    }
}
