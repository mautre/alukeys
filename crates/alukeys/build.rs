//! Embeds Windows resources without a resource compiler, so cross-compiling needs no
//! extra tools: the manifest goes through the linker, and the icon and version info are
//! written as a `.res` file that the linker accepts directly.

use std::path::{Path, PathBuf};

const ICON_SIZES: [u32; 8] = [16, 20, 24, 32, 40, 48, 64, 256];
const RT_ICON: u16 = 3;
const RT_GROUP_ICON: u16 = 14;
const RT_VERSION: u16 = 16;
const LANG_EN_US: u16 = 0x0409;

fn main() {
    let target = std::env::var("TARGET").unwrap_or_default();
    if !target.contains("windows-msvc") {
        return;
    }
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let manifest = dir.join("alukeys.manifest");
    println!("cargo:rerun-if-changed={}", manifest.display());
    println!("cargo:rustc-link-arg-bins=/MANIFEST:EMBED");
    println!("cargo:rustc-link-arg-bins=/MANIFESTINPUT:{}", manifest.display());
    println!("cargo:rustc-link-arg-bins=/MANIFESTUAC:NO");

    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("alukeys.res");
    std::fs::write(&out, resources(&dir.join("assets"))).expect("write .res");
    println!("cargo:rustc-link-arg-bins={}", out.display());
}

/// One entry of a .res file: 32-byte header with numeric type and name, then the data.
fn entry(res: &mut Vec<u8>, ty: u16, id: u16, flags: u16, data: &[u8]) {
    let push16 = |v: &mut Vec<u8>, x: u16| v.extend_from_slice(&x.to_le_bytes());
    let push32 = |v: &mut Vec<u8>, x: u32| v.extend_from_slice(&x.to_le_bytes());
    push32(res, data.len() as u32);
    push32(res, 32);
    push16(res, 0xFFFF);
    push16(res, ty);
    push16(res, 0xFFFF);
    push16(res, id);
    push32(res, 0); // data version
    push16(res, flags);
    push16(res, if ty == 0 { 0 } else { LANG_EN_US });
    push32(res, 0); // version
    push32(res, 0); // characteristics
    res.extend_from_slice(data);
    while !res.len().is_multiple_of(4) {
        res.push(0);
    }
}

fn resources(assets: &Path) -> Vec<u8> {
    let mut res = Vec::new();
    entry(&mut res, 0, 0, 0, &[]); // .res files start with an empty entry

    // Icons: PNG-compressed images (supported by Windows for every size).
    let mut group = vec![0, 0, 1, 0];
    group.extend_from_slice(&(ICON_SIZES.len() as u16).to_le_bytes());
    for (i, size) in ICON_SIZES.iter().enumerate() {
        let path = assets.join(format!("icon-{size}.png"));
        println!("cargo:rerun-if-changed={}", path.display());
        let png = std::fs::read(&path).expect("icon png (run tools/make_icon.py)");
        let id = i as u16 + 1;
        entry(&mut res, RT_ICON, id, 0x1010, &png);
        let dim = if *size >= 256 { 0 } else { *size as u8 };
        group.extend_from_slice(&[dim, dim, 0, 0]);
        group.extend_from_slice(&1u16.to_le_bytes()); // planes
        group.extend_from_slice(&32u16.to_le_bytes()); // bit count
        group.extend_from_slice(&(png.len() as u32).to_le_bytes());
        group.extend_from_slice(&id.to_le_bytes());
    }
    entry(&mut res, RT_GROUP_ICON, 1, 0x1030, &group);
    entry(&mut res, RT_VERSION, 1, 0x0030, &version_info());
    res
}

fn utf16z(s: &str) -> Vec<u8> {
    s.encode_utf16().chain(std::iter::once(0)).flat_map(|c| c.to_le_bytes()).collect()
}

fn pad4(v: &mut Vec<u8>) {
    while !v.len().is_multiple_of(4) {
        v.push(0);
    }
}

/// A VERSIONINFO block: length, value length, type, key, value, children (all aligned).
fn block(key: &str, value: &[u8], value_len: u16, text: bool, children: &[Vec<u8>]) -> Vec<u8> {
    let mut v = vec![0, 0];
    v.extend_from_slice(&value_len.to_le_bytes());
    v.extend_from_slice(&(text as u16).to_le_bytes());
    v.extend_from_slice(&utf16z(key));
    pad4(&mut v);
    v.extend_from_slice(value);
    for c in children {
        pad4(&mut v);
        v.extend_from_slice(c);
    }
    let len = v.len() as u16;
    v[..2].copy_from_slice(&len.to_le_bytes());
    v
}

fn version_info() -> Vec<u8> {
    let version = env!("CARGO_PKG_VERSION");
    let parts: Vec<u32> =
        version.split(['.', '-']).filter_map(|p| p.parse().ok()).chain([0, 0, 0, 0]).take(4).collect();
    let ms = parts[0] << 16 | parts[1];
    let ls = parts[2] << 16 | parts[3];
    let mut fixed = Vec::new();
    for x in [0xFEEF_04BDu32, 0x0001_0000, ms, ls, ms, ls, 0x3F, 0, 0x0004_0004, 1, 0, 0, 0] {
        fixed.extend_from_slice(&x.to_le_bytes());
    }

    let s = |k: &str, val: &str| {
        let data = utf16z(val);
        block(k, &data, (data.len() / 2) as u16, true, &[])
    };
    let strings = vec![
        s("CompanyName", "AluKeys contributors"),
        s("FileDescription", "AluKeys"),
        s("FileVersion", version),
        s("InternalName", "alukeys"),
        s("LegalCopyright", "MIT License"),
        s("OriginalFilename", "alukeys.exe"),
        s("ProductName", "AluKeys"),
        s("ProductVersion", version),
    ];
    let table = block("040904B0", &[], 0, true, &strings);
    let string_info = block("StringFileInfo", &[], 0, true, &[table]);
    let translation = [0x09u8, 0x04, 0xB0, 0x04];
    let var = block("Translation", &translation, 4, false, &[]);
    let var_info = block("VarFileInfo", &[], 0, true, &[var]);
    block("VS_VERSION_INFO", &fixed, fixed.len() as u16, false, &[string_info, var_info])
}
