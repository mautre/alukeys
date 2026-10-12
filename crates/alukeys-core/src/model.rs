//! Known Apple keyboards, identified by USB/Bluetooth product ID.

pub const APPLE_VID: u16 = 0x05AC;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Variant {
    Ansi,
    Iso,
    Jis,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Model {
    pub pid: u16,
    pub name: &'static str,
    pub variant: Variant,
    pub wireless: bool,
    pub numpad: bool,
}

const fn m(pid: u16, name: &'static str, variant: Variant, wireless: bool, numpad: bool) -> Model {
    Model { pid, name, variant, wireless, numpad }
}

pub const MODELS: &[Model] = &[
    m(0x022C, "Apple Wireless Keyboard (A1255)", Variant::Ansi, true, false),
    m(0x022D, "Apple Wireless Keyboard (A1255)", Variant::Iso, true, false),
    m(0x022E, "Apple Wireless Keyboard (A1255)", Variant::Jis, true, false),
    m(0x0239, "Apple Wireless Keyboard (A1314, 2009)", Variant::Ansi, true, false),
    m(0x023A, "Apple Wireless Keyboard (A1314, 2009)", Variant::Iso, true, false),
    m(0x023B, "Apple Wireless Keyboard (A1314, 2009)", Variant::Jis, true, false),
    m(0x0255, "Apple Wireless Keyboard (A1314, 2011)", Variant::Ansi, true, false),
    m(0x0256, "Apple Wireless Keyboard (A1314, 2011)", Variant::Iso, true, false),
    m(0x0257, "Apple Wireless Keyboard (A1314, 2011)", Variant::Jis, true, false),
    m(0x021D, "Apple Keyboard (A1243)", Variant::Ansi, false, true),
    m(0x021E, "Apple Keyboard (A1243)", Variant::Iso, false, true),
    m(0x021F, "Apple Keyboard (A1243)", Variant::Jis, false, true),
    m(0x0220, "Apple Keyboard (A1242)", Variant::Ansi, false, false),
    m(0x0221, "Apple Keyboard (A1242)", Variant::Iso, false, false),
    m(0x0222, "Apple Keyboard (A1242)", Variant::Jis, false, false),
    m(0x024F, "Apple Keyboard (A1243)", Variant::Ansi, false, true),
    m(0x0250, "Apple Keyboard (A1243)", Variant::Iso, false, true),
    m(0x0251, "Apple Keyboard (A1243)", Variant::Jis, false, true),
];

pub fn lookup(pid: u16) -> Option<&'static Model> {
    MODELS.iter().find(|m| m.pid == pid)
}
