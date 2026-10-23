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

/// The line under the graph: charging, or the use left at the recent pace.
pub fn summary(samples: &[Sample]) -> String {
    match (samples.last(), hours_left(samples)) {
        (Some(s), _) if s.charging => "Charging".into(),
        (_, Some(h)) if h >= 48.0 => format!("About {:.0} days of use left at your recent pace", h / 24.0),
        (_, Some(h)) => format!("About {h:.0} h of use left at your recent pace"),
        (Some(_), None) => "Time left shows once the battery has dropped a few percent since the last charge".into(),
        (None, None) => "History is recorded while the tray icon is running".into(),
    }
}

/// "24 h ago", "7 days ago": the left end of a graph spanning `span` seconds.
pub fn span_label(span: u64) -> String {
    if span >= 2 * DAY {
        format!("{} days ago", span / DAY)
    } else {
        format!("{} h ago", span / 3600)
    }
}

/// The settings window's graph in braille for the terminal: `cols` characters of
/// plot, `rows` lines tall, joined across time with no samples like the window.
pub fn plot(samples: &[Sample], span: u64, now: u64, cols: usize, rows: usize) -> String {
    let (w, h) = (cols * 2, rows * 4);
    let x_of = |t: u64| ((w - 1) as f64 * (1.0 - now.saturating_sub(t) as f64 / span as f64)).round() as isize;
    let y_of = |level: f64| (level.min(100.0) / 100.0 * (h - 1) as f64).round() as usize;
    let mut levels: Vec<Option<f64>> = vec![None; w];
    for (x, level) in levels.iter_mut().enumerate() {
        let t = now as f64 - span as f64 * (w - 1 - x) as f64 / (w - 1) as f64;
        let i = samples.partition_point(|s| (s.t as f64) < t);
        if i > 0 && i < samples.len() {
            let (a, b) = (samples[i - 1], samples[i]);
            let f = (t - a.t as f64) / (b.t - a.t).max(1) as f64;
            *level = Some(a.level as f64 + f * (b.level as f64 - a.level as f64));
        }
    }
    // samples themselves, so a lone reading still shows
    for s in samples {
        if let Ok(x) = usize::try_from(x_of(s.t)) {
            if x < w {
                levels[x] = Some(s.level as f64);
            }
        }
    }
    let mut dots = vec![vec![false; w]; h];
    let mut prev: Option<usize> = None;
    for (x, level) in levels.iter().enumerate() {
        let Some(level) = level else {
            prev = None;
            continue;
        };
        let y = y_of(*level);
        // fill the run from the last column so steep changes stay connected
        let (lo, hi) = prev.map_or((y, y), |p| (p.min(y), p.max(y)));
        for row in &mut dots[lo..=hi] {
            row[x] = true;
        }
        prev = Some(y);
    }
    const BITS: [[u32; 4]; 2] = [[0x01, 0x02, 0x04, 0x40], [0x08, 0x10, 0x20, 0x80]];
    let mut out = String::new();
    for r in 0..rows {
        let label = match r {
            0 => "100%",
            _ if r == rows / 2 => " 50%",
            _ if r == rows - 1 => "  0%",
            _ => "",
        };
        out += &format!("{label:>4} ┤");
        for c in 0..cols {
            let mut bits = 0;
            for (dx, col) in BITS.iter().enumerate() {
                for (dy, bit) in col.iter().enumerate() {
                    if dots[h - 1 - (r * 4 + dy)][c * 2 + dx] {
                        bits |= bit;
                    }
                }
            }
            out.push(char::from_u32(0x2800 + bits).unwrap_or(' '));
        }
        out.push('\n');
    }
    let start = span_label(span);
    out += &format!("      {start}{:>pad$}\n", "now", pad = cols.saturating_sub(start.chars().count()));
    out
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
    fn plot_joins_across_gaps() {
        let h = 3600;
        // 100% a day ago, 0% now, with no samples between
        let text = plot(&[s(0, 100, false), s(24 * h, 0, false)], DAY, 24 * h, 10, 2);
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 3);
        // every plot column has a dot, so the line runs the whole width
        for c in 0..10 {
            assert!(lines.iter().take(2).any(|l| l.chars().nth(6 + c) != Some('\u{2800}')), "{text}");
        }
        assert!(lines[2].ends_with("now"));
        let empty = plot(&[], DAY, 24 * h, 10, 2);
        assert!(empty.lines().take(2).all(|l| l.chars().skip(6).all(|c| c == '\u{2800}')));
    }

    #[test]
    fn ago_units() {
        assert_eq!(ago(30), "just now");
        assert_eq!(ago(600), "10 min ago");
        assert_eq!(ago(3 * 3600), "3 h ago");
        assert_eq!(ago(3 * DAY), "3 days ago");
    }
}
