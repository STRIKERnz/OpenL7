//! Battery level and settings for the MCHOSE L7 Pro on Linux.
//!
//!   openl7          show a system tray icon (StatusNotifierItem)
//!   openl7 --once   print the battery level and exit
//!   openl7 info     print the mouse's onboard settings
//!   openl7 gui      open the settings window

mod buttons;
mod gui;
mod hid;
mod icon;
mod prefs;
mod settings;

use std::io::ErrorKind;
use std::path::Path;
use std::process::{exit, Command};
use std::thread;
use std::time::Duration;

const TRAY_LOCK: &str = "openl7-tray.lock";
const POLL: Duration = Duration::from_secs(60);
/// How often the tray looks for preference changes made in the settings window.
const PREFS_POLL: Duration = Duration::from_secs(2);
/// Warn at this level, then again at every further 5% drop.
const LOW_BATTERY: u8 = 25;

#[derive(Clone, Copy)]
struct Battery {
    level: u8,
    charging: bool,
}

fn open_device() -> Result<hid::Device, String> {
    let path = hid::find_hidraw().ok_or("not found")?;
    hid::Device::open(&path).map_err(|e| match e.kind() {
        ErrorKind::PermissionDenied => format!("no permission for {path}"),
        _ => e.to_string(),
    })
}

fn status() -> Result<Battery, String> {
    let mut dev = open_device()?;
    // 0x11 0x06 device info: vid, pid, fw, connect, battery %, charge status
    match dev.query(0x11, &[0x06]) {
        Ok(Some(d)) if d[0..2] != [0, 0] => Ok(Battery { level: d[9], charging: d[10] != 0 }),
        Ok(_) => Err("asleep / not connected".into()),
        Err(e) => Err(e.to_string()),
    }
}

fn describe(b: &Battery) -> String {
    format!("L7 Pro: {}%{}", b.level, if b.charging { " (charging)" } else { "" })
}

// ---------------------------------------------------------------- tray icon

struct Tray {
    state: Result<Battery, String>,
    /// Active onboard profile and the profile names, refreshed with the battery.
    profile: Option<u8>,
    names: Vec<String>,
    /// Last profile switch failure, shown in the tooltip until the next success.
    switch_error: Option<String>,
    /// Lowest 5% step we've already warned about since last charge.
    warned_at: Option<u8>,
    prefs: prefs::Prefs,
}

impl Tray {
    fn refresh(&mut self) {
        self.state = status();
        if self.state.is_ok() {
            if let Ok(mut dev) = open_device() {
                self.profile = settings::active_profile(&mut dev).ok().flatten();
                if self.names.iter().all(String::is_empty) {
                    self.names = settings::profile_names(&mut dev).unwrap_or_default();
                }
            }
        }
        match &self.state {
            Ok(b) if b.level <= LOW_BATTERY && !b.charging => {
                let step = b.level / 5 * 5;
                if self.warned_at.is_none_or(|w| step < w) {
                    self.warned_at = Some(step);
                    let urgency = if b.level <= 10 { "critical" } else { "normal" };
                    let _ = Command::new("notify-send")
                        .args(["-u", urgency, "-i", "input-mouse-battery", "-a", "L7 Pro", "Mouse battery low"])
                        .arg(format!("{}% remaining", b.level))
                        .status();
                }
            }
            Ok(_) => self.warned_at = None,
            Err(_) => {}
        }
    }
}

impl Tray {
    fn profile_label(&self, i: u8) -> String {
        match self.names.get(i as usize) {
            Some(n) if !n.is_empty() => n.clone(),
            _ => format!("Profile {}", i + 1),
        }
    }

    fn switch(&mut self, index: u8) {
        let result = open_device().and_then(|mut d| settings::switch_profile(&mut d, index));
        self.switch_error = result.err();
        self.refresh();
    }
}

impl ksni::Tray for Tray {
    fn id(&self) -> String {
        "openl7".into()
    }

    fn category(&self) -> ksni::Category {
        ksni::Category::Hardware
    }

    fn title(&self) -> String {
        match &self.state {
            Ok(b) => describe(b),
            Err(_) => "L7 Pro".into(),
        }
    }

    fn icon_name(&self) -> String {
        match &self.state {
            Ok(_) if self.prefs.tray_percent => String::new(),
            Ok(b) => {
                let step = ((b.level as u32 + 5) / 10 * 10).min(100);
                format!("battery-{step:03}{}", if b.charging { "-charging" } else { "" })
            }
            Err(_) => "input-mouse".into(),
        }
    }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        match &self.state {
            Ok(b) if self.prefs.tray_percent => vec![icon::battery_number(b.level, b.charging)],
            _ => Vec::new(),
        }
    }

    fn tool_tip(&self) -> ksni::ToolTip {
        let mut description = match &self.state {
            Ok(b) if b.charging => "Charging".into(),
            Ok(_) => "On battery".into(),
            Err(e) => e.clone(),
        };
        if let (Ok(_), Some(p)) = (&self.state, self.profile) {
            description += &format!("\nProfile {}: {}", p + 1, self.profile_label(p));
        }
        if let Some(e) = &self.switch_error {
            description += &format!("\nProfile switch failed: {e}");
        }
        ksni::ToolTip { icon_name: self.icon_name(), icon_pixmap: self.icon_pixmap(), title: self.title(), description, ..Default::default() }
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        self.refresh();
    }

    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        use ksni::menu::{CheckmarkItem, RadioGroup, RadioItem, StandardItem, SubMenu};
        let mut items = Vec::new();
        if let (Ok(_), Some(active)) = (&self.state, self.profile) {
            items.push(
                SubMenu {
                    label: format!("Profile: {}", self.profile_label(active)),
                    icon_name: "user-identity".into(),
                    submenu: vec![RadioGroup {
                        selected: active as usize,
                        select: Box::new(|t: &mut Self, i| t.switch(i as u8)),
                        options: (0..settings::PROFILES)
                            .map(|i| RadioItem { label: format!("{}  {}", i + 1, self.profile_label(i)), ..Default::default() })
                            .collect(),
                    }
                    .into()],
                    ..Default::default()
                }
                .into(),
            );
            items.push(ksni::MenuItem::Separator);
        }
        items.extend([
            CheckmarkItem {
                label: "Show percentage on icon".into(),
                checked: self.prefs.tray_percent,
                activate: Box::new(|t: &mut Self| {
                    // reload first so the accent colour set in the settings window is kept
                    t.prefs = prefs::Prefs { tray_percent: !t.prefs.tray_percent, ..prefs::Prefs::load() };
                    let _ = t.prefs.save();
                }),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "Settings…".into(),
                icon_name: "configure".into(),
                activate: Box::new(|_| open_settings_window()),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "Refresh".into(),
                icon_name: "view-refresh".into(),
                activate: Box::new(|t: &mut Self| t.refresh()),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "Quit".into(),
                icon_name: "application-exit".into(),
                activate: Box::new(|_| exit(0)),
                ..Default::default()
            }
            .into(),
        ]);
        items
    }
}

/// Runs the settings window as its own process so the tray loop stays independent.
fn open_settings_window() {
    match std::env::current_exe() {
        Ok(exe) => {
            match Command::new(exe).arg("gui").spawn() {
                // reap it when it closes so no zombie is left behind
                Ok(mut child) => drop(thread::spawn(move || child.wait())),
                Err(e) => eprintln!("could not open settings: {e}"),
            }
        }
        Err(e) => eprintln!("could not open settings: {e}"),
    }
}

/// Starts the tray in the background unless one is already running, so launching
/// the settings window from the app menu brings the battery icon back too.
fn ensure_tray() {
    use std::os::unix::process::CommandExt;
    if gui::try_lock(TRAY_LOCK).is_none() {
        return;
    }
    // the lock is dropped here; the new tray takes it
    if let Ok(exe) = std::env::current_exe() {
        let _ = Command::new(exe).process_group(0).spawn();
    }
}

fn run_tray() {
    use ksni::blocking::TrayMethods;

    let Some(_lock) = gui::try_lock(TRAY_LOCK) else { return };

    let mut tray = Tray {
        state: Err("checking…".into()),
        profile: None,
        names: Vec::new(),
        switch_error: None,
        warned_at: None,
        prefs: prefs::Prefs::load(),
    };
    tray.refresh();
    let handle = match tray.spawn() {
        Ok(h) => h,
        Err(e) => {
            eprintln!("could not create tray icon: {e}");
            exit(1);
        }
    };
    let mut prefs_seen = prefs::modified();
    let mut since_refresh = Duration::ZERO;
    loop {
        thread::sleep(PREFS_POLL);
        since_refresh += PREFS_POLL;
        let now = prefs::modified();
        let prefs_changed = now != prefs_seen;
        prefs_seen = now;
        let due = since_refresh >= POLL;
        if due {
            since_refresh = Duration::ZERO;
        }
        if (due || prefs_changed)
            && handle
                .update(|t| {
                    if prefs_changed {
                        t.prefs = prefs::Prefs::load();
                    }
                    if due {
                        t.refresh();
                    }
                })
                .is_none()
        {
            exit(0);
        }
    }
}

const SET_HELP: &str = "settings (apply to the active profile):
  set dpi <stage 1-6> <dpi>     DPI of a stage (50-26000, steps of 50)
  set stage <1-6>               active DPI stage
  set rate <125|500|1000|2000|4000|8000>   polling rate in Hz (current connection)
  set debounce <0-30>           button debounce (ms)
  set sleep <0-60>              sleep after (minutes, 0 = never)
  set lod <1|2>                 lift-off distance (mm)
  set ripple <on|off>           ripple control
  set angle-snap <on|off>       angle snapping
  set motion-sync <on|off>      motion sync";

fn parse_num<T: std::str::FromStr>(s: Option<&String>, what: &str) -> Result<T, String> {
    s.ok_or(format!("missing {what}"))?.parse().map_err(|_| format!("bad {what}"))
}

fn parse_on_off(s: Option<&String>) -> Result<bool, String> {
    match s.map(String::as_str) {
        Some("on") => Ok(true),
        Some("off") => Ok(false),
        _ => Err("expected on or off".into()),
    }
}

/// `profile` lists the onboard profiles; `profile <1-3>` switches to one.
fn profile(arg: Option<&String>) -> Result<(), String> {
    let mut dev = open_device()?;
    let io = |e: std::io::Error| e.to_string();
    if let Some(a) = arg {
        let n: u8 = a.parse().map_err(|_| "profile must be 1-3")?;
        if !(1..=settings::PROFILES).contains(&n) {
            return Err("profile must be 1-3".into());
        }
        settings::switch_profile(&mut dev, n - 1)?;
    }
    let active = settings::active_profile(&mut dev).map_err(io)?.ok_or("asleep / not connected")?;
    for (i, name) in settings::profile_names(&mut dev).map_err(io)?.iter().enumerate() {
        println!("{} {}  {}", if i == active as usize { "*" } else { " " }, i + 1, name);
    }
    Ok(())
}

fn set(args: &[String]) -> Result<(), String> {
    let mut dev = open_device()?;
    let usb = settings::is_wired(&mut dev).map_err(|e| e.to_string())?.ok_or("asleep / not connected")?;
    let link = if usb { "USB" } else { "2.4G" };
    let mut cfg = settings::Config::read(&mut dev).map_err(|e| e.to_string())?.ok_or("asleep / not connected")?;
    let (what, a, b) = (args.first().map(String::as_str), args.get(1), args.get(2));
    let summary = match what {
        Some("dpi") => {
            let stage: usize = parse_num(a, "stage")?;
            let dpi: u16 = parse_num(b, "DPI")?;
            if !(1..=cfg.stages()).contains(&stage) {
                return Err(format!("stage must be 1-{}", cfg.stages()));
            }
            if !(settings::DPI_MIN..=settings::DPI_MAX).contains(&dpi) || dpi % 50 != 0 {
                return Err(format!("DPI must be {}-{} in steps of 50", settings::DPI_MIN, settings::DPI_MAX));
            }
            cfg.set_dpi(stage - 1, dpi);
            cfg.write_dpi(&mut dev, cfg.active_stage(usb))?;
            println!("L7 Pro: DPI stage {stage} = {dpi} (saved, verified)");
            return Ok(());
        }
        Some("stage") => {
            let stage: usize = parse_num(a, "stage")?;
            if !(1..=cfg.stages()).contains(&stage) {
                return Err(format!("stage must be 1-{}", cfg.stages()));
            }
            cfg.write_dpi(&mut dev, stage as u8 - 1)?;
            println!("L7 Pro: active DPI stage = {stage} ({})", cfg.dpi(stage - 1));
            return Ok(());
        }
        Some("rate") => {
            let hz: u32 = parse_num(a, "rate")?;
            let i = settings::RATES.iter().position(|&r| r == hz).ok_or("rate must be 125, 500, 1000, 2000, 4000 or 8000")?;
            cfg.set_rate(usb, i as u8);
            format!("polling rate ({link}) = {hz} Hz")
        }
        Some("debounce") => {
            let ms: u8 = parse_num(a, "debounce")?;
            if ms > 30 {
                return Err("debounce must be 0-30 ms".into());
            }
            cfg.set_debounce(ms);
            format!("debounce = {ms} ms")
        }
        Some("sleep") => {
            let min: u8 = parse_num(a, "minutes")?;
            if min > 60 {
                return Err("sleep must be 0-60 minutes (0 = never)".into());
            }
            cfg.set_sleep(min);
            if min == 0 { "sleep never".into() } else { format!("sleep after {min} min") }
        }
        Some("lod") => {
            let mm: u8 = parse_num(a, "lift-off")?;
            if mm != 1 && mm != 2 {
                return Err("lift-off must be 1 or 2".into());
            }
            cfg.set_lod(mm);
            format!("lift-off = {mm} mm")
        }
        Some(flag @ ("ripple" | "angle-snap" | "motion-sync")) => {
            let on = parse_on_off(a)?;
            let bit = match flag {
                "ripple" => 0x04,
                "angle-snap" => 0x08,
                _ => 0x10,
            };
            cfg.set_flag(bit, on);
            format!("{flag} {}", if on { "on" } else { "off" })
        }
        _ => return Err(format!("unknown setting\n\n{SET_HELP}")),
    };
    cfg.write(&mut dev)?;
    println!("L7 Pro: {summary} (saved, verified)");
    Ok(())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        None => run_tray(),
        Some("--once") => match status() {
            Ok(b) => println!("{}", describe(&b)),
            Err(e) => {
                println!("L7 Pro: {e}");
                exit(1);
            }
        },
        Some("gui") => {
            ensure_tray();
            if let Err(e) = gui::run() {
                eprintln!("L7 Pro: {e}");
                exit(1);
            }
        }
        Some("profile") => {
            if let Err(e) = profile(args.get(1)) {
                eprintln!("L7 Pro: {e}");
                exit(1);
            }
        }
        Some("set") => {
            if let Err(e) = set(&args[1..]) {
                eprintln!("L7 Pro: {e}");
                exit(1);
            }
        }
        Some("info") => {
            let report = open_device().and_then(|mut d| settings::dump(&mut d).map_err(|e| e.to_string()));
            match report {
                Ok(Some(text)) => print!("{text}"),
                Ok(None) => {
                    println!("L7 Pro: asleep / not connected");
                    exit(1);
                }
                Err(e) => {
                    println!("L7 Pro: {e}");
                    exit(1);
                }
            }
        }
        Some(_) => {
            let name = std::env::args().next().unwrap_or_default();
            let name = Path::new(&name).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            eprintln!(
                "usage: {name} [--once | info | gui | profile [1-3] | set <setting> <value>]\n  (no args)      show a system tray icon\n  --once         print battery and exit\n  gui            open the settings window\n  info           print onboard settings\n  profile [1-3]  list onboard profiles, or switch to one\n\n{SET_HELP}\n\nUnofficial tool, not affiliated with MCHOSE. Use at your own risk."
            );
            exit(2);
        }
    }
}
