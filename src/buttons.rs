//! Button actions as stored in the config block: a 4-bit type and a 3-byte value
//! (tables from M HUB's key-setting page).

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    Default,
    /// type 1: mouse function
    Mouse([u8; 3]),
    /// type 2: keyboard key, HID usage in the middle byte
    Key(u8),
    /// type 3: consumer/media key, usage in the first byte
    Media(u8),
    /// type 5: 1 = cycle DPI, 2 = DPI +, 3 = DPI -
    Dpi(u8),
    /// type 8: screen brightness
    Brightness([u8; 3]),
    /// type 9
    Disabled,
    /// anything else (macros, profile switching, …) — shown, kept as-is
    Other(u8, [u8; 3]),
}

pub const MOUSE: [([u8; 3], &str); 7] = [
    ([0x01, 0, 0], "Left click"),
    ([0x02, 0, 0], "Right click"),
    ([0x04, 0, 0], "Middle click"),
    ([0x10, 0, 0], "Forward"),
    ([0x08, 0, 0], "Back"),
    ([0x00, 0x02, 0], "Scroll up"),
    ([0x00, 0xfe, 0], "Scroll down"),
];

pub const MEDIA: [(u8, &str); 7] = [
    (0xe9, "Volume up"),
    (0xea, "Volume down"),
    (0xe2, "Mute"),
    (0xcd, "Play / pause"),
    (0xb6, "Previous track"),
    (0xb5, "Next track"),
    (0xb7, "Stop"),
];

pub const DPI: [(u8, &str); 3] = [(1, "DPI cycle"), (2, "DPI +"), (3, "DPI -")];

pub const BRIGHTNESS: [([u8; 3], &str); 2] = [([0x0c, 0x6f, 0], "Brightness +"), ([0x0c, 0x70, 0], "Brightness -")];

/// Keys offered in the picker, in display order.
pub fn key_codes() -> impl Iterator<Item = u8> {
    (0x04..=0x52u8).filter(|&c| c != 0x32).chain(0xe0..=0xe7)
}

pub fn key_name(code: u8) -> String {
    const MODS: [&str; 8] = ["Left Ctrl", "Left Shift", "Left Alt", "Left Super", "Right Ctrl", "Right Shift", "Right Alt", "Right Super"];
    const NAMED: [(u8, &str); 34] = [
        (0x28, "Enter"), (0x29, "Esc"), (0x2a, "Backspace"), (0x2b, "Tab"), (0x2c, "Space"),
        (0x2d, "- _"), (0x2e, "= +"), (0x2f, "[ {"), (0x30, "] }"), (0x31, "\\ |"), (0x33, "; :"),
        (0x34, "' \""), (0x35, "` ~"), (0x36, ", <"), (0x37, ". >"), (0x38, "/ ?"), (0x39, "Caps Lock"),
        (0x46, "Print Screen"), (0x47, "Scroll Lock"), (0x48, "Pause"), (0x49, "Insert"), (0x4a, "Home"),
        (0x4b, "Page Up"), (0x4c, "Delete"), (0x4d, "End"), (0x4e, "Page Down"), (0x4f, "Right"),
        (0x50, "Left"), (0x51, "Down"), (0x52, "Up"), (0x1e, "1 !"), (0x27, "0 )"), (0x32, "#"), (0x00, "None"),
    ];
    match code {
        0x04..=0x1d => ((b'A' + code - 0x04) as char).to_string(),
        0x1f..=0x26 => ((b'2' + code - 0x1f) as char).to_string(),
        0x3a..=0x45 => format!("F{}", code - 0x39),
        0xe0..=0xe7 => MODS[(code - 0xe0) as usize].into(),
        _ => NAMED.iter().find(|(c, _)| *c == code).map_or(format!("Key 0x{code:02x}"), |(_, n)| n.to_string()),
    }
}

impl Action {
    pub fn decode(kind: u8, v: [u8; 3]) -> Self {
        match kind {
            0 => Action::Default,
            1 => Action::Mouse(v),
            2 => Action::Key(v[1]),
            3 => Action::Media(v[0]),
            5 => Action::Dpi(v[0]),
            8 => Action::Brightness(v),
            9 => Action::Disabled,
            _ => Action::Other(kind, v),
        }
    }

    pub fn encode(self) -> (u8, [u8; 3]) {
        match self {
            Action::Default => (0, [0, 0, 0]),
            Action::Mouse(v) => (1, v),
            Action::Key(k) => (2, [0, k, 0]),
            Action::Media(m) => (3, [m, 0, 0]),
            Action::Dpi(d) => (5, [d, 0, 0]),
            Action::Brightness(v) => (8, v),
            Action::Disabled => (9, [0xff, 0xff, 0xff]),
            Action::Other(k, v) => (k, v),
        }
    }

    pub fn label(self) -> String {
        let find = |list: &[([u8; 3], &str)], v| list.iter().find(|(x, _)| *x == v).map(|(_, n)| n.to_string());
        match self {
            Action::Default => "Default".into(),
            Action::Mouse(v) => find(&MOUSE, v).unwrap_or(format!("Mouse function {v:02x?}")),
            Action::Key(k) => format!("Key: {}", key_name(k)),
            Action::Media(m) => MEDIA.iter().find(|(x, _)| *x == m).map_or(format!("Media 0x{m:02x}"), |(_, n)| n.to_string()),
            Action::Dpi(d) => DPI.iter().find(|(x, _)| *x == d).map_or(format!("DPI function {d}"), |(_, n)| n.to_string()),
            Action::Brightness(v) => find(&BRIGHTNESS, v).unwrap_or(format!("Brightness {v:02x?}")),
            Action::Disabled => "Disabled".into(),
            Action::Other(4, _) => "Macro".into(),
            Action::Other(10, v) => match v[0] {
                1..=3 => format!("Switch to profile {}", v[0]),
                4 => "Cycle profiles".into(),
                _ => format!("Profile function {v:02x?}"),
            },
            Action::Other(k, v) => format!("Type {k} {v:02x?}"),
        }
    }

    /// Every fixed choice offered in the picker (keyboard keys are picked separately).
    pub fn choices() -> Vec<Action> {
        let mut v = vec![Action::Default];
        v.extend(MOUSE.iter().map(|(x, _)| Action::Mouse(*x)));
        v.extend(MEDIA.iter().map(|(x, _)| Action::Media(*x)));
        v.extend(DPI.iter().map(|(x, _)| Action::Dpi(*x)));
        v.extend(BRIGHTNESS.iter().map(|(x, _)| Action::Brightness(*x)));
        v.push(Action::Disabled);
        v
    }
}
