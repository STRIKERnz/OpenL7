//! Profiles saved to disk: a name and the raw config block, one text file each in
//! `$XDG_CONFIG_HOME/openl7/profiles`.

use crate::settings::Config;
use std::path::PathBuf;

pub fn dir() -> PathBuf {
    crate::prefs::config_dir().join("profiles")
}

pub struct Saved {
    pub name: String,
    pub path: PathBuf,
}

/// Saved profiles, sorted by name.
pub fn list() -> Vec<Saved> {
    let Ok(entries) = std::fs::read_dir(dir()) else { return Vec::new() };
    let mut out: Vec<Saved> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "txt"))
        .filter_map(|path| Some(Saved { name: path.file_stem()?.to_string_lossy().into_owned(), path }))
        .collect();
    out.sort_by_key(|s| s.name.to_lowercase());
    out
}

/// Saves `cfg` under `name` (replacing a save with the same name) and returns the file.
pub fn save(name: &str, cfg: &Config) -> Result<PathBuf, String> {
    // keep the name usable as a file name
    let file: String = name.trim().chars().map(|c| if c == '/' || c.is_control() { '_' } else { c }).collect();
    if file.is_empty() || file.starts_with('.') {
        return Err("name can't be empty or start with a dot".into());
    }
    let hex: String = cfg.0[..63].iter().map(|b| format!("{b:02x}")).collect();
    let path = dir().join(format!("{file}.txt"));
    std::fs::create_dir_all(dir()).map_err(|e| e.to_string())?;
    std::fs::write(&path, format!("# OpenL7 saved profile (MCHOSE L7 Pro config block)\nconfig = {hex}\n"))
        .map_err(|e| e.to_string())?;
    Ok(path)
}

pub fn load(path: &std::path::Path) -> Result<Config, String> {
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let hex = text
        .lines()
        .find_map(|l| l.strip_prefix("config").and_then(|r| r.trim_start().strip_prefix('=')))
        .ok_or("not an OpenL7 profile file")?
        .trim();
    if hex.len() != 126 || !hex.is_ascii() {
        return Err("profile file is damaged".into());
    }
    let bytes = (0..63)
        .map(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16))
        .collect::<Result<Vec<u8>, _>>()
        .map_err(|_| "profile file is damaged")?;
    Ok(Config(bytes))
}

#[cfg(test)]
mod tests {
    #[test]
    fn round_trip() {
        let dir = std::env::temp_dir().join(format!("openl7-test-{}", std::process::id()));
        std::env::set_var("XDG_CONFIG_HOME", &dir);
        let cfg = crate::settings::Config((0..63).collect());
        let path = super::save("My / profile", &cfg).unwrap();
        assert!(path.ends_with("profiles/My _ profile.txt"));
        assert!(super::load(&path).unwrap() == cfg);
        assert_eq!(super::list().len(), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
