//! Transport for the MCHOSE vendor interface, as used by the M HUB web driver.
//!
//! Every byte after the report ID is XORed with 0xFF on the wire. A read is:
//!   1. SET_FEATURE report 0x11 (20-byte payload) or 0x12 (64-byte payload): [cmd, args…]
//!   2. the device acks with input report 0x13 whose decoded payload starts e5 <cmd>
//!   3. GET_FEATURE of the same report returns [cmd, data…]

use std::fs::{self, File, OpenOptions};
use std::io::{self, ErrorKind, Read};
use std::os::fd::AsRawFd;
use std::os::unix::fs::OpenOptionsExt;
use std::time::{Duration, Instant};

const VENDOR_ID: u16 = 0x5253;
const ACK_TIMEOUT: Duration = Duration::from_millis(600);
const ATTEMPTS: usize = 4;

/// The vendor-defined interface of the mouse/dongle (the one with report 0x11).
pub fn find_hidraw() -> Option<String> {
    let mut entries: Vec<_> = fs::read_dir("/sys/class/hidraw").ok()?.flatten().map(|e| e.path()).collect();
    entries.sort();
    let id = format!("HID_ID=0003:0000{VENDOR_ID:04X}:");
    for path in entries {
        let dev = path.join("device");
        let Ok(uevent) = fs::read_to_string(dev.join("uevent")) else { continue };
        let Ok(desc) = fs::read(dev.join("report_descriptor")) else { continue };
        if !uevent.contains(&id) {
            continue;
        }
        // vendor usage page (06 xx ff) and a report ID 0x11 item (85 11)
        if desc.len() > 3 && desc[0] == 0x06 && desc[2] == 0xff && desc.windows(2).any(|w| w == [0x85, 0x11]) {
            return Some(format!("/dev/{}", path.file_name()?.to_string_lossy()));
        }
    }
    None
}

pub struct Device {
    file: File,
    /// flock held for the lifetime of the Device so the tray's polling can't
    /// interleave with a read/write from the CLI or settings window.
    _lock: Option<File>,
}

fn lock() -> Option<File> {
    let dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".into());
    let f = OpenOptions::new().create(true).write(true).truncate(false).open(format!("{dir}/l7-battery.lock")).ok()?;
    unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX) };
    Some(f)
}

fn ioc_rw(nr: u64, size: usize) -> u64 {
    (3 << 30) | ((size as u64) << 16) | ((b'H' as u64) << 8) | nr
}

impl Device {
    pub fn open(path: &str) -> io::Result<Self> {
        let _lock = lock();
        let file = OpenOptions::new().read(true).write(true).custom_flags(libc::O_NONBLOCK).open(path)?;
        Ok(Self { file, _lock })
    }

    fn wait_readable(&self, timeout: Duration) -> bool {
        let mut pfd = libc::pollfd { fd: self.file.as_raw_fd(), events: libc::POLLIN, revents: 0 };
        let ms = timeout.as_millis().min(i32::MAX as u128) as i32;
        unsafe { libc::poll(&mut pfd, 1, ms) > 0 }
    }

    fn ioctl(&self, nr: u64, buf: &mut [u8]) -> io::Result<()> {
        let r = unsafe { libc::ioctl(self.file.as_raw_fd(), ioc_rw(nr, buf.len()) as _, buf.as_mut_ptr()) };
        if r < 0 { Err(io::Error::last_os_error()) } else { Ok(()) }
    }

    fn drain(&mut self) {
        let mut scratch = [0u8; 64];
        while self.wait_readable(Duration::ZERO) && self.file.read(&mut scratch).is_ok() {}
    }

    fn wait_ack(&mut self, cmd: u8) -> io::Result<bool> {
        let mut buf = [0u8; 64];
        let deadline = Instant::now() + ACK_TIMEOUT;
        while let Some(left) = deadline.checked_duration_since(Instant::now()) {
            if !self.wait_readable(left) {
                break;
            }
            let n = match self.file.read(&mut buf) {
                Ok(n) => n,
                Err(e) if e.kind() == ErrorKind::WouldBlock => continue,
                Err(e) => return Err(e),
            };
            if n >= 3 && buf[0] == 0x13 && buf[1] ^ 0xff == 0xe5 && buf[2] ^ 0xff == cmd {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Sends a write command and waits for the e5 ack. Returns false if it never came.
    pub fn send(&mut self, report: u8, data: &[u8]) -> io::Result<bool> {
        let len = if report == 0x11 { 20 } else { 64 };
        let mut req = vec![0xffu8; len + 1];
        req[0] = report;
        for (dst, b) in req[1..].iter_mut().zip(data) {
            *dst = b ^ 0xff;
        }
        self.drain();
        self.ioctl(0x06, &mut req)?; // HIDIOCSFEATURE
        self.wait_ack(data[0])
    }

    /// Runs one read command. Returns the decoded reply after the command byte,
    /// or None if the mouse didn't answer (asleep/off).
    pub fn query(&mut self, report: u8, cmd: &[u8]) -> io::Result<Option<Vec<u8>>> {
        let len = if report == 0x11 { 20 } else { 64 };
        let mut req = vec![0xffu8; len + 1];
        req[0] = report;
        for (dst, b) in req[1..].iter_mut().zip(cmd) {
            *dst = b ^ 0xff;
        }
        for _ in 0..ATTEMPTS {
            self.drain();
            self.ioctl(0x06, &mut req.clone())?; // HIDIOCSFEATURE
            if !self.wait_ack(cmd[0])? {
                continue;
            }
            let mut buf = vec![0u8; len + 1];
            buf[0] = report;
            self.ioctl(0x07, &mut buf)?; // HIDIOCGFEATURE
            let d: Vec<u8> = buf[1..].iter().map(|b| b ^ 0xff).collect();
            if d[0] == cmd[0] {
                return Ok(Some(d[1..].to_vec()));
            }
        }
        Ok(None)
    }
}
