//! Button actions as stored in the config block: a 4-bit type and a 3-byte value
//! (types and tables from M HUB's key-setting page).

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    Default,
    /// type 1: mouse function
    Mouse([u8; 3]),
    /// type 2: keyboard key, HID usage in the middle byte
    Key(u8),
    /// type 3: media key, consumer usage in the first byte
    Media(u8),
    /// type 5: 1 = cycle DPI, 2 = DPI +, 3 = DPI -
    Dpi(u8),
    /// type 8 with usage page 0x07: modifier mask + key ("Hotkey" in M HUB;
    /// Copy is `07 01 06`, i.e. Left Ctrl + C)
    Hotkey { mods: u8, key: u8 },
    /// type 8 with another usage page, e.g. `0c 6f 00` screen brightness +
    System([u8; 3]),
    /// type 9
    Disabled,
    /// anything else (macros, profile switching, …): shown and kept as-is
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

pub const SYSTEM: [([u8; 3], &str); 2] = [([0x0c, 0x6f, 0], "Brightness +"), ([0x0c, 0x70, 0], "Brightness -")];

/// M HUB's hotkey presets.
pub const HOTKEY_PRESETS: [(u8, u8, &str); 3] = [(0x01, 0x06, "Copy"), (0x01, 0x1b, "Cut"), (0x01, 0x19, "Paste")];

/// HID modifier bits, as used in the hotkey's modifier byte.
pub const CTRL: u8 = 0x01;
pub const SHIFT: u8 = 0x02;
pub const ALT: u8 = 0x04;
pub const SUPER: u8 = 0x08;

/// Keyboard keys by group, for the picker.
pub fn key_groups() -> Vec<(&'static str, Vec<u8>)> {
    vec![
        ("Letters", (0x04..=0x1d).collect()),
        ("Numbers", (0x1e..=0x27).collect()),
        ("Function keys", (0x3a..=0x45).chain(0x68..=0x73).collect()),
        ("Navigation", vec![0x52, 0x51, 0x50, 0x4f, 0x4a, 0x4d, 0x4b, 0x4e, 0x49, 0x4c]),
        ("Editing", vec![0x28, 0x29, 0x2b, 0x2c, 0x2a, 0x39, 0x46, 0x47, 0x48, 0x65]),
        ("Symbols", vec![0x2d, 0x2e, 0x2f, 0x30, 0x31, 0x33, 0x34, 0x35, 0x36, 0x37, 0x38]),
        ("Numpad", (0x59..=0x63).chain(0x54..=0x58).collect()),
        ("Modifiers", (0xe0..=0xe7).collect()),
    ]
}

pub fn key_name(code: u8) -> String {
    const MODS: [&str; 8] = ["Left Ctrl", "Left Shift", "Left Alt", "Left Super", "Right Ctrl", "Right Shift", "Right Alt", "Right Super"];
    const NAMED: [(u8, &str); 39] = [
        (0x27, "0"), (0x28, "Enter"), (0x29, "Esc"), (0x2a, "Backspace"), (0x2b, "Tab"), (0x2c, "Space"),
        (0x2d, "-"), (0x2e, "="), (0x2f, "["), (0x30, "]"), (0x31, "\\"), (0x33, ";"),
        (0x34, "'"), (0x35, "`"), (0x36, ","), (0x37, "."), (0x38, "/"), (0x39, "Caps Lock"),
        (0x46, "Print Screen"), (0x47, "Scroll Lock"), (0x48, "Pause"), (0x49, "Insert"), (0x4a, "Home"),
        (0x4b, "Page Up"), (0x4c, "Delete"), (0x4d, "End"), (0x4e, "Page Down"), (0x4f, "Right"),
        (0x50, "Left"), (0x51, "Down"), (0x52, "Up"), (0x54, "Num /"), (0x55, "Num *"), (0x56, "Num -"),
        (0x57, "Num +"), (0x58, "Num Enter"), (0x62, "Num 0"), (0x63, "Num ."), (0x65, "Menu"),
    ];
    match code {
        0x04..=0x1d => ((b'A' + code - 0x04) as char).to_string(),
        0x1e..=0x26 => ((b'1' + code - 0x1e) as char).to_string(),
        0x3a..=0x45 => format!("F{}", code - 0x39),
        0x68..=0x73 => format!("F{}", code - 0x68 + 13),
        0x59..=0x61 => format!("Num {}", code - 0x58),
        0xe0..=0xe7 => MODS[(code - 0xe0) as usize].into(),
        _ => NAMED.iter().find(|(c, _)| *c == code).map_or(format!("Key 0x{code:02x}"), |(_, n)| n.to_string()),
    }
}

/// "Ctrl + Shift + T". Left and right modifiers are shown alike.
pub fn combo_name(mods: u8, key: u8) -> String {
    let mut parts = Vec::new();
    for (bit, name) in [(CTRL, "Ctrl"), (SHIFT, "Shift"), (ALT, "Alt"), (SUPER, "Super")] {
        if mods & (bit | bit << 4) != 0 {
            parts.push(name.to_string());
        }
    }
    parts.push(key_name(key));
    parts.join(" + ")
}

/// HID usage for a key egui reports by name (used when recording a hotkey).
pub fn hid_from_key_name(name: &str) -> Option<u8> {
    let b = name.as_bytes();
    if b.len() == 1 {
        let c = b[0];
        return match c {
            b'A'..=b'Z' => Some(0x04 + c - b'A'),
            b'a'..=b'z' => Some(0x04 + c - b'a'),
            b'1'..=b'9' => Some(0x1e + c - b'1'),
            b'0' => Some(0x27),
            b'-' => Some(0x2d),
            b'=' => Some(0x2e),
            b'[' => Some(0x2f),
            b']' => Some(0x30),
            b'\\' => Some(0x31),
            b';' => Some(0x33),
            b'\'' => Some(0x34),
            b'`' => Some(0x35),
            b',' => Some(0x36),
            b'.' => Some(0x37),
            b'/' => Some(0x38),
            _ => None,
        };
    }
    if let Some(n) = name.strip_prefix('F').and_then(|n| n.parse::<u8>().ok()) {
        return match n {
            1..=12 => Some(0x39 + n),
            13..=24 => Some(0x68 + n - 13),
            _ => None,
        };
    }
    Some(match name {
        "Enter" => 0x28,
        "Escape" => 0x29,
        "Backspace" => 0x2a,
        "Tab" => 0x2b,
        "Space" => 0x2c,
        "Insert" => 0x49,
        "Home" => 0x4a,
        "PageUp" => 0x4b,
        "Delete" => 0x4c,
        "End" => 0x4d,
        "PageDown" => 0x4e,
        "Right" | "ArrowRight" => 0x4f,
        "Left" | "ArrowLeft" => 0x50,
        "Down" | "ArrowDown" => 0x51,
        "Up" | "ArrowUp" => 0x52,
        _ => return None,
    })
}

impl Action {
    pub fn decode(kind: u8, v: [u8; 3]) -> Self {
        match kind {
            0 => Action::Default,
            1 => Action::Mouse(v),
            2 => Action::Key(v[1]),
            3 => Action::Media(v[0]),
            5 => Action::Dpi(v[0]),
            8 if v[0] == 0x07 => Action::Hotkey { mods: v[1], key: v[2] },
            8 => Action::System(v),
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
            Action::Hotkey { mods, key } => (8, [0x07, mods, key]),
            Action::System(v) => (8, v),
            Action::Disabled => (9, [0xff, 0xff, 0xff]),
            Action::Other(k, v) => (k, v),
        }
    }

    pub fn label(self) -> String {
        let find = |list: &[([u8; 3], &str)], v| list.iter().find(|(x, _)| *x == v).map(|(_, n)| n.to_string());
        match self {
            Action::Default => "Default".into(),
            Action::Mouse(v) => find(&MOUSE, v).unwrap_or(format!("Mouse function {v:02x?}")),
            Action::Key(k) => key_name(k),
            Action::Media(m) => MEDIA.iter().find(|(x, _)| *x == m).map_or(format!("Media 0x{m:02x}"), |(_, n)| n.to_string()),
            Action::Dpi(d) => DPI.iter().find(|(x, _)| *x == d).map_or(format!("DPI function {d}"), |(_, n)| n.to_string()),
            Action::Hotkey { mods, key } => match HOTKEY_PRESETS.iter().find(|(m, k, _)| (*m, *k) == (mods, key)) {
                Some((_, _, name)) => format!("{name} ({})", combo_name(mods, key)),
                None => combo_name(mods, key),
            },
            Action::System(v) => find(&SYSTEM, v).unwrap_or(format!("System {v:02x?}")),
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

    /// Short category name shown next to the label.
    pub fn category(self) -> &'static str {
        match self {
            Action::Default => "",
            Action::Mouse(_) => "Mouse",
            Action::Key(_) => "Key",
            Action::Media(_) => "Media",
            Action::Dpi(_) => "DPI",
            Action::Hotkey { .. } => "Hotkey",
            Action::System(_) => "System",
            Action::Disabled => "",
            Action::Other(4, _) => "Macro",
            Action::Other(10, _) => "Profile",
            Action::Other(..) => "Other",
        }
    }
}

/// Command-line name for a table label: "Play / pause" -> "play-pause", "DPI +" -> "dpi-up".
fn slug(label: &str) -> String {
    label.replace(" / ", "-").replace(" +", "-up").replace(" -", "-down").replace(' ', "-").to_lowercase()
}

/// The named actions `parse` accepts, besides keys and key combos.
pub fn action_names() -> Vec<String> {
    let mut names = vec!["default".to_string(), "disabled".to_string()];
    names.extend(MOUSE.iter().map(|(_, n)| slug(n)));
    names.extend(MEDIA.iter().map(|(_, n)| slug(n)));
    names.extend(DPI.iter().map(|(_, n)| slug(n)));
    names.extend(SYSTEM.iter().map(|(_, n)| slug(n)));
    names
}

/// HID usage for a key by its display name, ignoring case, spaces and dashes
/// ("f13", "page-down", "left-shift").
fn key_by_name(name: &str) -> Option<u8> {
    let norm = |s: &str| if s == "-" { s.to_string() } else { s.to_lowercase().replace([' ', '-'], "") };
    let want = norm(name);
    (0x04..=0xe7u8).find(|&c| norm(&key_name(c)) == want)
}

/// Parses a command-line action: a named one (see `action_names`), a key ("f13"),
/// or a combo ("ctrl+shift+t").
pub fn parse(s: &str) -> Option<Action> {
    let s = s.trim().to_lowercase();
    match s.as_str() {
        "default" => return Some(Action::Default),
        "disabled" => return Some(Action::Disabled),
        _ => {}
    }
    if let Some((v, _)) = MOUSE.iter().find(|(_, n)| slug(n) == s) {
        return Some(Action::Mouse(*v));
    }
    if let Some((v, _)) = MEDIA.iter().find(|(_, n)| slug(n) == s) {
        return Some(Action::Media(*v));
    }
    if let Some((v, _)) = DPI.iter().find(|(_, n)| slug(n) == s) {
        return Some(Action::Dpi(*v));
    }
    if let Some((v, _)) = SYSTEM.iter().find(|(_, n)| slug(n) == s) {
        return Some(Action::System(*v));
    }
    let mut parts: Vec<&str> = s.split('+').collect();
    let key = key_by_name(parts.pop()?)?;
    let mut mods = 0;
    for m in parts {
        mods |= match m {
            "ctrl" | "control" => CTRL,
            "shift" => SHIFT,
            "alt" => ALT,
            "super" | "meta" | "win" => SUPER,
            _ => return None,
        };
    }
    Some(if mods == 0 { Action::Key(key) } else { Action::Hotkey { mods, key } })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_cli_actions() {
        assert_eq!(parse("ctrl+shift+t"), Some(Action::Hotkey { mods: CTRL | SHIFT, key: 0x17 }));
        assert_eq!(parse("F13"), Some(Action::Key(0x68)));
        assert_eq!(parse("left-shift"), Some(Action::Key(0xe1)));
        assert_eq!(parse("page-down"), Some(Action::Key(0x4e)));
        assert_eq!(parse("play-pause"), Some(Action::Media(0xcd)));
        assert_eq!(parse("dpi-up"), Some(Action::Dpi(2)));
        assert_eq!(parse("middle-click"), Some(Action::Mouse([0x04, 0, 0])));
        assert_eq!(parse("brightness-down"), Some(Action::System([0x0c, 0x70, 0])));
        assert_eq!(parse("disabled"), Some(Action::Disabled));
        assert_eq!(parse("hyper+t"), None);
        assert_eq!(parse("nonsense"), None);
        for name in action_names() {
            assert!(parse(&name).is_some(), "{name}");
        }
    }

    #[test]
    fn round_trips_and_m_hub_values() {
        // Copy is M HUB's 0x070106 with type 8
        assert_eq!(Action::Hotkey { mods: CTRL, key: 0x06 }.encode(), (8, [0x07, 0x01, 0x06]));
        assert_eq!(Action::decode(8, [0x0c, 0x6f, 0]), Action::System([0x0c, 0x6f, 0]));
        for a in [
            Action::Default,
            Action::Mouse([0x04, 0, 0]),
            Action::Key(0xe1),
            Action::Media(0xcd),
            Action::Dpi(2),
            Action::Hotkey { mods: CTRL | SHIFT, key: 0x17 },
            Action::Disabled,
        ] {
            let (k, v) = a.encode();
            assert_eq!(Action::decode(k, v), a);
        }
        assert_eq!(combo_name(CTRL | SHIFT, 0x17), "Ctrl + Shift + T");
        assert_eq!(hid_from_key_name("T"), Some(0x17));
        assert_eq!(hid_from_key_name("F13"), Some(0x68));
        assert_eq!(hid_from_key_name("ArrowUp"), Some(0x52));
    }
}
