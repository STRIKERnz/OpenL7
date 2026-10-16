//! Read-only decoding of the mouse's onboard settings (layouts from the M HUB web driver).

use crate::buttons::Action;
use crate::hid::Device;
use std::fmt::Write;
use std::io;

/// Polling rates by rate index (L7 Pro: up to 8000 Hz on the 2.4G dongle and over USB).
pub const RATES: [u32; 6] = [125, 500, 1000, 2000, 4000, 8000];
pub const BUTTONS: [&str; 6] = ["Left", "Middle", "Right", "Forward", "Back", "DPI"];

fn u16le(d: &[u8], i: usize) -> u16 {
    u16::from_le_bytes([d[i], d[i + 1]])
}

/// Config byte holding a connection's rate (high nibble) and active DPI stage (low).
/// M HUB's read parser says byte 1 is 2.4G, but its write schema and the hardware
/// say USB: with byte 1 at 2000 Hz the dongle measured 1000 Hz, byte 2's value.
fn link_byte(usb: bool) -> usize {
    if usb { 1 } else { 2 }
}

fn rate(i: u8) -> String {
    RATES.get(i as usize).map_or(format!("index {i}"), |r| format!("{r} Hz"))
}

/// Profile names are stored NUL-terminated UTF-8 after the index byte. M HUB
/// appends `$<source>$<id>` to names of profiles it downloaded; that part is hidden.
fn profile_name(d: &[u8]) -> String {
    let s = &d[1..];
    let end = s.iter().position(|&b| b == 0).unwrap_or(s.len());
    let name = String::from_utf8_lossy(&s[..end]);
    name.split('$').next().unwrap_or_default().to_string()
}

/// Longest name the 0x12 0x59 frame holds: 64 bytes less the command and index.
pub const NAME_MAX: usize = 62;

/// Renames an onboard profile with 0x12 0x59 (as M HUB's profile editor does),
/// then reads the name back to confirm.
pub fn rename_profile(dev: &mut Device, index: u8, name: &str) -> Result<(), String> {
    let io = |e: io::Error| e.to_string();
    let name = name.trim();
    if name.is_empty() || name.len() > NAME_MAX || name.contains('$') {
        return Err(format!("name must be 1-{NAME_MAX} bytes, without $"));
    }
    let mut packet = vec![0x59, index];
    packet.extend_from_slice(name.as_bytes());
    if !dev.send(0x12, &packet).map_err(io)? {
        return Err("mouse didn't acknowledge the rename".into());
    }
    std::thread::sleep(std::time::Duration::from_millis(200)); // M HUB waits too
    match dev.query(0x12, &[0x68, index]).map_err(io)? {
        Some(d) if profile_name(&d) == name => Ok(()),
        Some(d) => Err(format!("mouse kept the name \"{}\"", profile_name(&d))),
        None => Err("couldn't read the name back".into()),
    }
}

pub const PROFILES: u8 = 3;

/// Names of the onboard profiles (empty string if one couldn't be read).
pub fn profile_names(dev: &mut Device) -> io::Result<Vec<String>> {
    (0..PROFILES).map(|i| Ok(dev.query(0x12, &[0x68, i])?.map(|d| profile_name(&d)).unwrap_or_default())).collect()
}

/// Index of the active onboard profile.
pub fn active_profile(dev: &mut Device) -> io::Result<Option<u8>> {
    Ok(Config::read(dev)?.map(|c| c.0[0]))
}

/// Switches the onboard profile with 0x11 0x58 (M HUB's profile picker), then
/// confirms it by reading the config back.
pub fn switch_profile(dev: &mut Device, index: u8) -> Result<(), String> {
    let io = |e: io::Error| e.to_string();
    if !dev.send(0x11, &[0x58, index]).map_err(io)? {
        return Err("mouse didn't acknowledge the profile switch".into());
    }
    std::thread::sleep(std::time::Duration::from_millis(100)); // M HUB waits too
    match active_profile(dev).map_err(io)? {
        Some(p) if p == index => Ok(()),
        Some(p) => Err(format!("mouse is still on profile {}", p + 1)),
        None => Err("couldn't read the profile back".into()),
    }
}

/// True when connected by cable rather than the 2.4G dongle (0x11 0x06 connectMode 0).
pub struct DeviceInfo {
    pub battery: u8,
    pub charging: bool,
    pub wired: bool,
    pub firmware: String,
}

pub fn device_info(dev: &mut Device) -> io::Result<Option<DeviceInfo>> {
    let Some(info) = dev.query(0x11, &[0x06])? else { return Ok(None) };
    let firmware = dev.query(0x11, &[0x04])?.map(|d| {
        let n = (d[0] as usize).min(d.len() - 1);
        String::from_utf8_lossy(&d[1..1 + n]).into_owned()
    });
    Ok(Some(DeviceInfo {
        battery: info[9],
        charging: info[10] != 0,
        wired: info[8] & 0x07 == 0,
        firmware: firmware.unwrap_or_default(),
    }))
}

pub fn is_wired(dev: &mut Device) -> io::Result<Option<bool>> {
    Ok(dev.query(0x11, &[0x06])?.map(|d| d[8] & 0x07 == 0))
}

pub fn dump(dev: &mut Device) -> io::Result<Option<String>> {
    let mut out = String::new();
    let Some(info) = dev.query(0x11, &[0x06])? else { return Ok(None) };

    let version = dev.query(0x11, &[0x04])?.map(|d| {
        let n = (d[0] as usize).min(d.len() - 1);
        String::from_utf8_lossy(&d[1..1 + n]).into_owned()
    });
    let bond = dev.query(0x11, &[0x03])?;
    let _ = writeln!(out, "Device");
    let _ = writeln!(out, "  firmware        {}", version.as_deref().unwrap_or("?"));
    let _ = writeln!(out, "  receiver        {:04x}:{:04x}", u16le(&info, 0), u16le(&info, 2));
    if let Some(b) = &bond {
        let _ = writeln!(out, "  paired mouse    {:04x}:{:04x}", u16le(b, 1), u16le(b, 3));
    }
    let _ = writeln!(out, "  battery         {}%{}", info[9], if info[10] != 0 { " (charging)" } else { "" });

    let names = profile_names(dev)?;

    let Some(c) = dev.query(0x12, &[0x67])? else {
        let _ = writeln!(out, "\n(settings read timed out)");
        return Ok(Some(out));
    };
    let profile = c[0];
    let (g_dpi, g_rate) = (c[link_byte(false)] & 0x0f, c[link_byte(false)] >> 4);
    let (usb_dpi, usb_rate) = (c[link_byte(true)] & 0x0f, c[link_byte(true)] >> 4);
    let stages = (c[16] as usize).clamp(1, 6);
    let (sensor, debounce, sleep, rotate) = (c[17], c[18], c[19], c[49] as i8);

    let _ = writeln!(out, "\nProfile {} of 3: {}", profile + 1, names.get(profile as usize).map_or("", |s| s));
    for (i, n) in names.iter().enumerate() {
        let _ = writeln!(out, "  {} {}  {}", if i == profile as usize { "*" } else { " " }, i + 1, n);
    }

    let _ = writeln!(out, "\nDPI stages (* = active on 2.4G, u = active on USB)");
    for i in 0..stages {
        let x = u16le(&c, 4 + i * 2);
        let y = u16le(&c, 51 + i * 2);
        let mark = format!("{}{}", if i == g_dpi as usize { "*" } else { " " }, if i == usb_dpi as usize { "u" } else { " " });
        let yinfo = if y != x && y != 0 { format!("  (Y {y})") } else { String::new() };
        let _ = writeln!(out, "  {mark} {}  {x}{yinfo}", i + 1);
    }

    let lod = match sensor & 0b11 {
        0b01 => "1 mm",
        0b10 => "2 mm",
        _ => "?",
    };
    let on = |bit: u8| if sensor & bit != 0 { "on" } else { "off" };
    let _ = writeln!(out, "\nPerformance");
    let _ = writeln!(out, "  polling rate    {} (2.4G), {} (USB)", rate(g_rate), rate(usb_rate));
    let _ = writeln!(out, "  lift-off        {lod}");
    let _ = writeln!(out, "  ripple control  {}", on(0x04));
    let _ = writeln!(out, "  angle snapping  {}", on(0x08));
    let _ = writeln!(out, "  motion sync     {}", on(0x10));
    let _ = writeln!(out, "  debounce        {debounce} ms");
    let _ = writeln!(out, "  sleep after     {}", if sleep == 0 { "never".into() } else { format!("{sleep} min") });
    let _ = writeln!(out, "  sensor mode     {}", SENSOR_MODES[Config(c.clone()).sensor_mode() as usize]);
    let _ = writeln!(out, "  sensor angle    {rotate}°");
    let _ = writeln!(out, "  sensor flags    0x{sensor:02x}");

    let _ = writeln!(out, "\nButtons");
    for (i, name) in BUTTONS.iter().enumerate() {
        let o = 20 + i * 4;
        let action = Action::decode(c[o] & 0x0f, [c[o + 1], c[o + 2], c[o + 3]]);
        let _ = writeln!(out, "  {name:<8} {}", action.label());
    }
    Ok(Some(out))
}

// ---------------------------------------------------------------- writing

/// The 63-byte 0x67 config block. 0x57 writes the identical layout back.
#[derive(Clone, PartialEq)]
pub struct Config(pub Vec<u8>);

/// Sensor modes as M HUB names them for the L7 Pro (sensor byte bits 6-7).
pub const SENSOR_MODES: [&str; 3] = ["Performance", "Gaming", "Extreme"];
pub const ANGLE_MAX: i8 = 30;
pub const DEBOUNCE_MAX: u8 = 30;
pub const SLEEP_MAX: u8 = 60;

pub const DPI_MIN: u16 = 50;
pub const DPI_MAX: u16 = 26000;

impl Config {
    pub fn read(dev: &mut Device) -> io::Result<Option<Self>> {
        Ok(dev.query(0x12, &[0x67])?.filter(|c| c.len() >= 63).map(Config))
    }

    pub fn stages(&self) -> usize {
        (self.0[16] as usize).clamp(1, 6)
    }

    /// Index of the onboard profile this config belongs to.
    pub fn profile(&self) -> u8 {
        self.0[0]
    }

    pub fn set_stages(&mut self, n: usize) {
        self.0[16] = n.clamp(1, 6) as u8;
    }

    /// Selects the active DPI stage for both connections (as M HUB does).
    pub fn set_active_stage(&mut self, stage: u8) {
        for i in [1, 2] {
            self.0[i] = (self.0[i] & 0xf0) | stage;
        }
    }

    pub fn rate(&self, usb: bool) -> u8 {
        self.0[link_byte(usb)] >> 4
    }

    pub fn debounce(&self) -> u8 {
        self.0[18]
    }

    pub fn sleep(&self) -> u8 {
        self.0[19]
    }

    pub fn lod(&self) -> u8 {
        if self.0[17] & 0b11 == 0b01 { 1 } else { 2 }
    }

    pub fn flag(&self, bit: u8) -> bool {
        self.0[17] & bit != 0
    }

    /// 0 Performance, 1 Gaming (bit 7), 2 Extreme (bits 7 and 6).
    pub fn sensor_mode(&self) -> u8 {
        match self.0[17] >> 6 {
            0b10 => 1,
            0b11 => 2,
            _ => 0,
        }
    }

    pub fn set_sensor_mode(&mut self, mode: u8) {
        let bits = [0b00, 0b10, 0b11][mode.min(2) as usize];
        self.0[17] = (self.0[17] & 0x3f) | (bits << 6);
    }

    pub fn angle(&self) -> i8 {
        self.0[49] as i8
    }

    pub fn set_angle(&mut self, deg: i8) {
        self.0[49] = deg.clamp(-ANGLE_MAX, ANGLE_MAX) as u8;
    }

    pub fn button(&self, i: usize) -> Action {
        let o = 20 + i * 4;
        Action::decode(self.0[o] & 0x0f, [self.0[o + 1], self.0[o + 2], self.0[o + 3]])
    }

    pub fn set_button(&mut self, i: usize, action: Action) {
        let o = 20 + i * 4;
        let (kind, v) = action.encode();
        self.0[o] = (self.0[o] & 0xf0) | kind;
        self.0[o + 1..o + 4].copy_from_slice(&v);
    }

    /// Writes whatever differs from `orig`: the 0x57 block for everything but the
    /// active stage, then 0x40 if the DPI table or active stage changed.
    pub fn apply(&self, orig: &Config, dev: &mut Device) -> Result<Config, String> {
        let mut block = self.clone();
        block.0[1] = (block.0[1] & 0xf0) | (orig.0[1] & 0x0f);
        block.0[2] = (block.0[2] & 0xf0) | (orig.0[2] & 0x0f);
        block.0[3] = orig.0[3]; // mirrors the active stage; maintained by the mouse
        let mut now = orig.clone();
        if block.0[..50] != orig.0[..50] || block.0[51..63] != orig.0[51..63] {
            now = block.write(dev)?;
        }
        let stage = self.active_stage(false);
        if self.0[4..17] != now.0[4..17] || stage != now.active_stage(false) || stage != now.active_stage(true) {
            now = self.write_dpi(dev, stage)?;
        }
        Ok(now)
    }

    pub fn dpi(&self, stage: usize) -> u16 {
        u16le(&self.0, 4 + stage * 2)
    }

    pub fn set_dpi(&mut self, stage: usize, dpi: u16) {
        let [lo, hi] = dpi.to_le_bytes();
        self.0[4 + stage * 2..6 + stage * 2].copy_from_slice(&[lo, hi]);
        // Y-axis DPI follows X (M HUB does the same when separate Y is off)
        self.0[51 + stage * 2..53 + stage * 2].copy_from_slice(&[lo, hi]);
    }

    pub fn active_stage(&self, usb: bool) -> u8 {
        self.0[link_byte(usb)] & 0x0f
    }

    /// Writes the DPI table and selects `stage` with 0x12 0x40, as M HUB's DPI page
    /// does (it sets the same stage for 2.4G and USB). The active stage is live state
    /// that the 0x57 block write doesn't change.
    pub fn write_dpi(&self, dev: &mut Device, stage: u8) -> Result<Config, String> {
        let io = |e: io::Error| e.to_string();
        let mut packet = vec![0x40, stage, stage, 0x00];
        packet.extend_from_slice(&self.0[4..16]); // dpi0-5
        packet.push(self.0[16]); // number of stages
        packet.push(0xff); // `diff`, always 255
        packet.extend_from_slice(&self.0[51..63]); // Y dpi0-5
        if !dev.send(0x12, &packet).map_err(io)? {
            return Err("mouse didn't acknowledge the write".into());
        }
        let after = Config::read(dev).map_err(io)?.ok_or("couldn't read settings back")?;
        if after.0[4..16] != self.0[4..16] || after.active_stage(false) != stage || after.active_stage(true) != stage {
            return Err("mouse reports different DPI settings after the write".into());
        }
        Ok(after)
    }

    /// Polling rate index for the 2.4G or USB connection.
    pub fn set_rate(&mut self, usb: bool, index: u8) {
        let b = &mut self.0[link_byte(usb)];
        *b = (*b & 0x0f) | (index << 4);
    }

    pub fn set_debounce(&mut self, ms: u8) {
        self.0[18] = ms;
    }

    pub fn set_sleep(&mut self, minutes: u8) {
        self.0[19] = minutes;
    }

    pub fn set_lod(&mut self, mm: u8) {
        self.0[17] = (self.0[17] & !0b11) | if mm == 1 { 0b01 } else { 0b10 };
    }

    pub fn set_flag(&mut self, bit: u8, on: bool) {
        if on { self.0[17] |= bit } else { self.0[17] &= !bit }
    }

    pub fn left_button_default(&self) -> bool {
        self.0[20] & 0x0f == 0 && self.0[21..24] == [0, 0, 0]
    }

    /// Writes the block back the way M HUB's setMouseAllInfo does, then reads it
    /// back to confirm. Returns the config as the mouse now reports it.
    pub fn write(&self, dev: &mut Device) -> Result<Config, String> {
        // M HUB treats a non-default left button as corruption; never write one
        if !self.left_button_default() {
            return Err("left button isn't default in this config; refusing to write".into());
        }
        let io = |e: io::Error| e.to_string();
        dev.send(0x11, &[0x02, 0x00]).map_err(io)?;
        let mut packet = Vec::with_capacity(64);
        packet.push(0x57);
        packet.extend_from_slice(&self.0[..63]);
        packet[1 + 50] = 0xff; // `val`, set to 255 by M HUB before every write
        if !dev.send(0x12, &packet).map_err(io)? {
            return Err("mouse didn't acknowledge the write".into());
        }
        let after = Config::read(dev).map_err(io)?.ok_or("couldn't read settings back")?;
        let same = |a: &[u8], b: &[u8]| a[..50] == b[..50] && a[51..63] == b[51..63];
        if !same(&after.0, &self.0) {
            return Err("mouse reports different settings after the write".into());
        }
        Ok(after)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Talks to the real mouse; run with `cargo test -- --ignored`.
    /// Changes several settings in one apply, then restores and checks every byte.
    #[test]
    #[ignore]
    fn hardware_apply_round_trip() {
        let path = crate::hid::find_hidraw().expect("mouse not found");
        let mut dev = Device::open(&path).unwrap();
        let orig = Config::read(&mut dev).unwrap().expect("no config");

        let mut edit = orig.clone();
        edit.set_button(4, Action::Hotkey { mods: 0x03, key: 0x17 }); // Back -> Ctrl+Shift+T
        edit.set_button(3, Action::Media(0xe9)); // Forward -> Volume up
        edit.set_sensor_mode(1);
        edit.set_angle(5);
        edit.set_stages(5);
        edit.set_active_stage(1);
        edit.set_dpi(4, 3000);
        let now = edit.apply(&orig, &mut dev).expect("apply");
        assert_eq!(now.button(4), Action::Hotkey { mods: 0x03, key: 0x17 });
        assert_eq!(now.button(3), Action::Media(0xe9));
        assert_eq!((now.sensor_mode(), now.angle(), now.stages()), (1, 5, 5));
        assert_eq!((now.active_stage(false), now.active_stage(true), now.dpi(4)), (1, 1, 3000));

        let back = orig.apply(&now, &mut dev).expect("restore");
        let same = |a: &Config, b: &Config| a.0[..50] == b.0[..50] && a.0[51..63] == b.0[51..63];
        assert!(same(&back, &orig), "restore mismatch:\n{:02x?}\n{:02x?}", back.0, orig.0);
    }

    /// Renames profile 3 (multi-byte UTF-8 included), then puts the old name back.
    #[test]
    #[ignore]
    fn hardware_rename_round_trip() {
        let path = crate::hid::find_hidraw().expect("mouse not found");
        let mut dev = Device::open(&path).unwrap();
        let orig = profile_names(&mut dev).unwrap()[2].clone();
        rename_profile(&mut dev, 2, "OpenL7 tést").expect("rename");
        assert_eq!(profile_names(&mut dev).unwrap()[2], "OpenL7 tést");
        rename_profile(&mut dev, 2, &orig).expect("restore");
        assert_eq!(profile_names(&mut dev).unwrap()[2], orig);
    }
}
