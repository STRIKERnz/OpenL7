//! App preferences (not mouse settings), shared by the tray and the settings window
//! through `$XDG_CONFIG_HOME/openl7/prefs`.

use std::path::PathBuf;
use std::time::SystemTime;

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Prefs {
    pub accent: [u8; 3],
    /// Draw the battery level as a number on the tray icon.
    pub tray_percent: bool,
}

impl Default for Prefs {
    fn default() -> Self {
        Self { accent: [0x5b, 0x8c, 0xff], tray_percent: false }
    }
}

/// `$XDG_CONFIG_HOME/openl7`.
pub fn config_dir() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".config"));
    base.join("openl7")
}

fn path() -> PathBuf {
    config_dir().join("prefs")
}

/// Modification time, so the tray can notice the settings window changing a pref.
pub fn modified() -> Option<SystemTime> {
    std::fs::metadata(path()).and_then(|m| m.modified()).ok()
}

impl Prefs {
    pub fn load() -> Self {
        let mut p = Self::default();
        let Ok(text) = std::fs::read_to_string(path()) else { return p };
        for line in text.lines() {
            match line.split_once('=').map(|(k, v)| (k.trim(), v.trim())) {
                Some(("accent", v)) => {
                    if let Some(c) = parse_hex(v) {
                        p.accent = c;
                    }
                }
                Some(("tray_percent", v)) => p.tray_percent = v == "true",
                _ => {}
            }
        }
        p
    }

    pub fn save(&self) -> std::io::Result<()> {
        let path = path();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let [r, g, b] = self.accent;
        let text = format!("accent = #{r:02x}{g:02x}{b:02x}\ntray_percent = {}\n", self.tray_percent);
        // write then rename, so the tray never reads a half-written file
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, text)?;
        std::fs::rename(tmp, path)
    }
}

fn parse_hex(s: &str) -> Option<[u8; 3]> {
    let s = s.strip_prefix('#')?;
    if s.len() != 6 {
        return None;
    }
    let n = u32::from_str_radix(s, 16).ok()?;
    Some([(n >> 16) as u8, (n >> 8) as u8, n as u8])
}

#[cfg(test)]
mod tests {
    #[test]
    fn hex() {
        assert_eq!(super::parse_hex("#783ecd"), Some([0x78, 0x3e, 0xcd]));
        assert_eq!(super::parse_hex("783ecd"), None);
    }
}
