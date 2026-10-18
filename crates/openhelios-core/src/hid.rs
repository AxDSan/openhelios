//! Vendor HID channel for the 0d62:9a1b keyboard.
//!
//! Functional HID command facts identified for interoperability (see
//! docs/PROVENANCE.md); no vendor code is copied.
//!
//! * feature report: report id `0`, then an 8-byte command whose byte 7 is
//!   `~sum(bytes[0..6])`; the report is 9 bytes in total;
//! * bulk colour data: 512 bytes = 128 slots of `[0, R, G, B]`, sent as eight
//!   65-byte output reports (report id `0` + 64 payload bytes);
//! * static mode sequence: command `12 00 00 08 …`, data, command
//!   `08 02 33 05 <brightness> 08 01 …`;
//! * effect sequence: `08 02 …`, `14 00 00 R G B …`,
//!   `08 00 <effect> <abs(speed-10)> <brightness> <1|8> <direction> …`.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::thread::sleep;
use std::time::Duration;

use crate::error::{Error, Result};
use crate::protocol::Rgb;

const VID_PID: &str = "0003:0D62:9A1B";
const REPORT_LEN: usize = 9;
const DATA_LEN: usize = 512;

const fn ioc(dir: u64, ty: u64, nr: u64, size: u64) -> u64 {
    (dir << 30) | (size << 16) | (ty << 8) | nr
}

const fn hidiocsfeature(len: usize) -> libc::c_ulong {
    ioc(3, b'H' as u64, 0x06, len as u64) as libc::c_ulong
}

const fn hidiocgfeature(len: usize) -> libc::c_ulong {
    ioc(3, b'H' as u64, 0x07, len as u64) as libc::c_ulong
}

#[derive(Debug)]
struct Candidate {
    path: PathBuf,
    phys: String,
    score: i32,
}

#[derive(Debug)]
pub struct HidKeyboard {
    file: File,
    path: PathBuf,
    phys: String,
    health: [u8; 8],
}

impl HidKeyboard {
    /// Locate the `0xFF01` vendor interface and prove it answers the getter.
    pub fn open() -> Result<Self> {
        let mut candidates = discover()?;
        candidates.sort_by_key(|c| -c.score);

        let mut permission: Option<PathBuf> = None;
        let mut last_err: Option<Error> = None;

        for cand in candidates {
            let file = match OpenOptions::new().read(true).write(true).open(&cand.path) {
                Ok(f) => f,
                Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                    permission.get_or_insert_with(|| cand.path.clone());
                    continue;
                }
                Err(e) => {
                    last_err = Some(Error::Io(e));
                    continue;
                }
            };

            let mut kb = Self {
                file,
                path: cand.path.clone(),
                phys: cand.phys.clone(),
                health: [0; 8],
            };
            match kb.get_cmd([0x88, 0, 0, 0, 0, 0, 0, 0]) {
                Ok(resp) if resp[0] == 0x88 => {
                    kb.health = resp;
                    return Ok(kb);
                }
                Ok(_) | Err(_) => continue,
            }
        }

        if let Some(path) = permission {
            return Err(Error::Permission { path });
        }
        Err(last_err.unwrap_or(Error::HidUnavailable))
    }

    pub fn open_path(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .map_err(|e| match e.kind() {
                std::io::ErrorKind::PermissionDenied => Error::Permission { path: path.clone() },
                _ => Error::Io(e),
            })?;
        let mut kb = Self {
            file,
            path,
            phys: String::new(),
            health: [0; 8],
        };
        kb.health = kb.get_cmd([0x88, 0, 0, 0, 0, 0, 0, 0])?;
        Ok(kb)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn phys(&self) -> &str {
        &self.phys
    }

    pub fn health(&self) -> [u8; 8] {
        self.health
    }

    fn ioctl_feature(&self, request: libc::c_ulong, buf: &mut [u8]) -> Result<()> {
        let rc = unsafe { libc::ioctl(self.file.as_raw_fd(), request, buf.as_mut_ptr()) };
        if rc < 0 {
            return Err(Error::Io(std::io::Error::last_os_error()));
        }
        Ok(())
    }

    /// Send one 8-byte command. Byte 7 is overwritten with the checksum.
    pub fn send_cmd(&self, mut cmd: [u8; 8]) -> Result<()> {
        cmd[7] = checksum(&cmd[..7]);
        let mut frame = [0u8; REPORT_LEN];
        frame[0] = 0;
        frame[1..9].copy_from_slice(&cmd);
        self.ioctl_feature(hidiocsfeature(REPORT_LEN), &mut frame)
    }

    /// Send a command, then read the 8-byte feature response.
    pub fn get_cmd(&self, cmd: [u8; 8]) -> Result<[u8; 8]> {
        self.send_cmd(cmd)?;
        let mut buf = [0u8; REPORT_LEN];
        self.ioctl_feature(hidiocgfeature(REPORT_LEN), &mut buf)?;
        let mut out = [0u8; 8];
        out.copy_from_slice(&buf[1..9]);
        Ok(out)
    }

    fn write_data(&self, data: &[u8]) -> Result<()> {
        if data.len() != DATA_LEN {
            return Err(Error::Hid(format!(
                "colour buffer must be {DATA_LEN} bytes, got {}",
                data.len()
            )));
        }
        let mut f = &self.file;
        for chunk in data.chunks(64) {
            let mut report = [0u8; 65];
            report[0] = 0;
            report[1..1 + chunk.len()].copy_from_slice(chunk);
            f.write_all(&report)?;
        }
        f.flush()?;
        Ok(())
    }

    /// Static per-key colours. `colors[126]` is the touchpad slot.
    pub fn static_colors(&self, colors: &[Rgb; 128], brightness: u8) -> Result<()> {
        self.send_cmd([0x12, 0, 0, 0x08, 0, 0, 0, 0])?;
        sleep(Duration::from_millis(100));

        let mut data = [0u8; DATA_LEN];
        for (i, c) in colors.iter().enumerate() {
            data[i * 4 + 1] = c.r;
            data[i * 4 + 2] = c.g;
            data[i * 4 + 3] = c.b;
        }
        self.write_data(&data)?;
        sleep(Duration::from_millis(100));

        self.send_cmd([0x08, 0x02, 0x33, 0x05, brightness.min(100), 0x08, 0x01, 0])?;
        sleep(Duration::from_millis(100));
        Ok(())
    }

    /// Effect mode. `speed` is 0..=10 as exposed by PredatorSense.
    pub fn effect(
        &self,
        effect: u8,
        speed: u8,
        brightness: u8,
        direction: u8,
        color: Rgb,
    ) -> Result<()> {
        if effect == 0 && speed == 0 && direction == 0 {
            self.send_cmd([0x08, 0x01, 0, 0, 0, 0, 0, 0])?;
        } else {
            self.send_cmd([0x08, 0x02, 0, 0, 0, 0, 0, 0])?;
            self.send_cmd([0x14, 0, 0, color.r, color.g, color.b, 0, 0])?;
            let speed = (speed as i16 - 10).unsigned_abs() as u8;
            let tail = if effect == 9 { 8 } else { 1 };
            self.send_cmd([
                0x08,
                0x00,
                effect,
                speed,
                brightness.min(100),
                tail,
                direction,
                0,
            ])?;
        }
        sleep(Duration::from_millis(100));
        Ok(())
    }

    /// Brightness-only update, using PredatorSense's `(51, 5, 1)` triple.
    pub fn brightness(&self, brightness: u8) -> Result<()> {
        self.effect(51, 5, brightness, 1, Rgb::BLACK)
    }

    /// MagForce per-key lighting (keys with adjustable actuation).
    ///
    /// `rainbow` is PredatorSense's second argument: `1` single colour, `8`
    /// rainbow. This path is present on the PH717 (`MagForce=1`).
    pub fn magforce(&self, _effect: u8, rainbow: u8, color: Rgb, brightness: u8) -> Result<()> {
        self.send_cmd([0x08, 0x02, 0, 0, 0, 0, 0, 0])?;
        self.send_cmd([0x14, 0, 0, color.r, color.g, color.b, 0, 0])?;
        let tail = if rainbow == 8 { 8 } else { 1 };
        self.send_cmd([0x08, 0x00, 0x18, 0x01, brightness.min(100), tail, 0, 0])?;
        sleep(Duration::from_millis(100));
        Ok(())
    }

    /// MagForce switch/linear/joystick setting (agent command 0x31).
    pub fn set_key_setting(&self, switch_mode: u8, linear_mode: u8, joystick: u8) -> Result<()> {
        self.send_cmd([0x31, switch_mode, linear_mode, joystick, 0, 0, 0, 0])
    }

    pub fn get_key_setting(&self) -> Result<[u8; 8]> {
        self.get_cmd([0xB1, 0, 0, 0, 0, 0, 0, 0])
    }

    /// Actuation point (agent command 0x32).
    pub fn set_actuation_point(&self, value: u8) -> Result<()> {
        self.send_cmd([0x32, value, 0, 0, 0, 0, 0, 0])
    }

    pub fn get_actuation_point(&self) -> Result<[u8; 8]> {
        self.get_cmd([0xB2, 0, 0, 0, 0, 0, 0, 0])
    }

    /// Analog curve mode (agent command 0x35).
    pub fn set_analog_curve(&self, mode: u8) -> Result<()> {
        self.send_cmd([0x35, mode, 0, 0, 0, 0, 0, 0])
    }

    pub fn get_analog_curve(&self) -> Result<[u8; 8]> {
        self.get_cmd([0xB5, 0, 0, 0, 0, 0, 0, 0])
    }

    /// MagForce replace mode (agent command 0x30, always 1 in byte 1).
    pub fn set_magforce_replace_mode(&self, status: bool, status_30s: bool) -> Result<()> {
        self.send_cmd([0x30, 0x01, status_30s as u8, status as u8, 0, 0, 0, 0])
    }

    /// 30-second keyboard backlight getter.
    pub fn get_backlight_30s(&self) -> Result<[u8; 8]> {
        self.get_cmd([0xB0, 0, 0, 0, 0, 0, 0, 0])
    }

    /// Game-key map getter (agent command 0xB4).
    pub fn get_gamekeys(&self) -> Result<[u8; 8]> {
        self.get_cmd([0xB4, 0, 0, 0, 0, 0, 0, 0])
    }

    /// Assign up to four keys to W/A/S/D game-key usages.
    ///
    /// The agent sends a 0x34/0x19 feature header followed by a 25-byte output
    /// report whose four selected slots contain the HID usages 0x1a, 0x04,
    /// 0x16 and 0x07.
    pub fn set_gamekeys(&self, keys: [u8; 4]) -> Result<()> {
        self.send_cmd([0x34, 0, 0, 0x19, 0, 0, 0, 0])?;
        let usages = [0x1au8, 0x04, 0x16, 0x07];
        let mut payload = [0u8; 25];
        for (i, key) in keys.iter().enumerate() {
            let idx = *key as usize;
            if idx < payload.len() {
                payload[idx] = usages[i];
            }
        }
        let mut report = [0u8; 65];
        report[0] = 0;
        report[1..26].copy_from_slice(&payload);
        let mut f = &self.file;
        f.write_all(&report)?;
        f.flush()?;
        sleep(Duration::from_millis(200));
        Ok(())
    }

    pub fn off(&self) -> Result<()> {
        self.static_colors(&[Rgb::BLACK; 128], 0)
    }

    /// PredatorSense getter 0x88. Byte 3 is used as a boolean by the agent,
    /// byte 4 is returned numerically by `GetUSBActuationPoint`-style calls.
    pub fn status(&self) -> Result<[u8; 8]> {
        self.get_cmd([0x88, 0, 0, 0, 0, 0, 0, 0])
    }

    pub fn status_b0(&self) -> Result<[u8; 8]> {
        self.get_cmd([0xB0, 0, 0, 0, 0, 0, 0, 0])
    }

    /// Raw feature command, for diagnostics and future MagForce work.
    pub fn raw_cmd(&self, cmd: [u8; 8]) -> Result<()> {
        self.send_cmd(cmd)
    }

    pub fn raw_get(&self, cmd: [u8; 8]) -> Result<[u8; 8]> {
        self.get_cmd(cmd)
    }
}

pub fn checksum(bytes: &[u8]) -> u8 {
    !bytes.iter().fold(0u8, |a, b| a.wrapping_add(*b))
}

fn discover() -> Result<Vec<Candidate>> {
    let dir = Path::new("/sys/bus/hid/devices");
    let entries =
        fs::read_dir(dir).map_err(|e| Error::Hid(format!("cannot scan {}: {e}", dir.display())))?;
    let mut out = Vec::new();

    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.to_ascii_uppercase().contains(VID_PID) {
            continue;
        }
        let devdir = entry.path();
        let phys = fs::read_to_string(devdir.join("uevent"))
            .ok()
            .and_then(|s| {
                s.lines()
                    .find_map(|l| l.strip_prefix("HID_PHYS=").map(str::to_string))
            })
            .unwrap_or_default();

        let mut score = if phys.ends_with("/input3") {
            100
        } else if phys.ends_with("/input1") {
            50
        } else if phys.ends_with("/input2") {
            40
        } else {
            10
        };

        if let Ok(desc) = fs::read(devdir.join("report_descriptor")) {
            if desc.windows(3).any(|w| w == [0x06, 0x01, 0xFF]) {
                score += 100;
            }
        }

        let hidraw_dir = devdir.join("hidraw");
        if let Ok(list) = fs::read_dir(&hidraw_dir) {
            for h in list.flatten() {
                let hname = h.file_name();
                let hname = hname.to_string_lossy();
                if hname.starts_with("hidraw") {
                    out.push(Candidate {
                        path: PathBuf::from("/dev").join(hname.as_ref()),
                        phys: phys.clone(),
                        score,
                    });
                }
            }
        }
    }

    if out.is_empty() {
        return Err(Error::HidUnavailable);
    }
    Ok(out)
}
