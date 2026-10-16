//! Battery history the tray records for the settings window's graph: one
//! `unix-seconds level charging` line per change in `$XDG_STATE_HOME/openl7/battery`.

use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

/// Write a sample this often even when nothing changed, so the graph can tell a
/// steady level from the mouse being asleep or the tray not running.
const HEARTBEAT: u64 = 10 * 60;
/// Samples further apart than this have no data between them.
pub const GAP: u64 = HEARTBEAT * 2;
const KEEP: u64 = 30 * 24 * 3600;
const DAY: u64 = 24 * 3600;

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Sample {
    pub t: u64,
    pub level: u8,
    pub charging: bool,
}

fn path() -> PathBuf {
    let base = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".local/state"));
    base.join("openl7").join("battery")
}

pub fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// Modification time, so the settings window can notice new samples.
pub fn modified() -> Option<SystemTime> {
    std::fs::metadata(path()).and_then(|m| m.modified()).ok()
}

fn parse(text: &str) -> Vec<Sample> {
    text.lines()
        .filter_map(|line| {
            let mut f = line.split_whitespace();
            Some(Sample { t: f.next()?.parse().ok()?, level: f.next()?.parse().ok()?, charging: f.next()? == "1" })
        })
        .collect()
}

fn format(s: &Sample) -> String {
    format!("{} {} {}\n", s.t, s.level, s.charging as u8)
}

pub fn load() -> Vec<Sample> {
    parse(&std::fs::read_to_string(path()).unwrap_or_default())
}

/// Appends the tray's readings, skipping repeats between heartbeats, and drops
/// samples older than `KEEP` once a day.
pub struct Recorder {
    last: Option<Sample>,
    pruned: u64,
}

impl Recorder {
    pub fn new() -> Self {
        Self { last: load().last().copied(), pruned: 0 }
    }

    pub fn record(&mut self, level: u8, charging: bool) {
        let t = now();
        if self.last.is_some_and(|l| l.level == level && l.charging == charging && t < l.t + HEARTBEAT) {
            return;
        }
        let s = Sample { t, level, charging };
        if append(&s).is_ok() {
            self.last = Some(s);
        }
        if t >= self.pruned + DAY {
            self.pruned = t;
            let _ = prune(t);
        }
    }
}

fn append(s: &Sample) -> std::io::Result<()> {
    let path = path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::OpenOptions::new().create(true).append(true).open(path)?.write_all(format(s).as_bytes())
}

fn prune(now: u64) -> std::io::Result<()> {
    let samples = load();
    let keep: Vec<_> = samples.iter().filter(|s| s.t + KEEP >= now).collect();
    if keep.len() == samples.len() {
        return Ok(());
    }
    // write then rename, so the settings window never reads a half-written file
    let path = path();
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, keep.iter().map(|s| format(s)).collect::<String>())?;
    std::fs::rename(tmp, path)
}

/// Hours of use left at the drain rate since the mouse last came off the charger.
/// Time with no samples (mouse asleep, tray not running) isn't counted as use.
pub fn hours_left(samples: &[Sample]) -> Option<f64> {
    let last = *samples.last()?;
    if last.charging {
        return None;
    }
    // walk back to the start of this discharge; a jump of a few percent means it
    // was charged while nobody was watching, smaller ones are reading noise
    let mut start = samples.len() - 1;
    while start > 0 && !samples[start - 1].charging && samples[start - 1].level + 2 >= samples[start].level {
        start -= 1;
    }
    let run = &samples[start..];
    let secs: u64 = run.windows(2).map(|w| w[1].t.saturating_sub(w[0].t)).filter(|&d| d <= GAP).sum();
    let dropped = run[0].level.saturating_sub(last.level);
    let hours = secs as f64 / 3600.0;
    // the level only moves in whole percents, so short runs say little
    if dropped < 3 || hours < 1.0 {
        return None;
    }
    Some(last.level as f64 * hours / dropped as f64)
}

/// "5 min ago", "3 h ago", "2 days ago".
pub fn ago(secs: u64) -> String {
    match secs {
        0..120 => "just now".into(),
        ..7200 => format!("{} min ago", secs / 60),
        ..172800 => format!("{} h ago", secs / 3600),
        _ => format!("{} days ago", secs / DAY),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(t: u64, level: u8, charging: bool) -> Sample {
        Sample { t, level, charging }
    }

    #[test]
    fn round_trip() {
        let samples = vec![s(1_700_000_000, 91, false), s(1_700_000_600, 100, true)];
        let text: String = samples.iter().map(format).collect();
        assert_eq!(parse(&text), samples);
        assert_eq!(parse("garbage\n12 x 0\n5 50 0\n"), vec![s(5, 50, false)]);
    }

    #[test]
    fn hours_left_since_last_charge() {
        let h = 3600;
        let step = HEARTBEAT;
        // charged to 100, then 10% over 30 heartbeats with a 3 h sleep after 12 of them
        let mut samples = vec![s(0, 80, false), s(step, 100, true)];
        for i in 0..=30 {
            let sleep = if i > 12 { 3 * h } else { 0 };
            samples.push(s(2 * step + i * step + sleep, 100 - (i / 3) as u8, false));
        }
        samples[20].level += 1; // reading noise
        let left = hours_left(&samples).unwrap();
        // 29 counted heartbeats (the sleep isn't use) for 10%, at 90% left
        assert!((left - 90.0 * 29.0 / 6.0 / 10.0).abs() < 0.01, "{left}");
        assert_eq!(hours_left(&samples[..5]), None, "nothing dropped yet");
        assert_eq!(hours_left(&[s(0, 50, false), s(h, 90, true)]), None, "charging");
        // a jump up without a charging sample still starts a new run
        assert_eq!(hours_left(&[s(0, 20, false), s(h, 10, false), s(2 * h, 95, false)]), None);
    }

    #[test]
    fn ago_units() {
        assert_eq!(ago(30), "just now");
        assert_eq!(ago(600), "10 min ago");
        assert_eq!(ago(3 * 3600), "3 h ago");
        assert_eq!(ago(3 * DAY), "3 days ago");
    }
}
