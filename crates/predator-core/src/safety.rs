//! Writes are gated, logged and snapshotted. Recovery is a first-class path.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::config;
use crate::controller::Status;
use crate::error::{Error, Result};
use crate::profile::Profile;

#[derive(Clone, Debug)]
pub struct SafetyPolicy {
    /// Global write switch. Read-only commands work when this is false.
    pub allow_writes: bool,
    /// Separate switch for EC overclock writes.
    pub allow_oc: bool,
    /// Show what would be written without touching hardware.
    pub dry_run: bool,
    /// Allow unknown/raw method ids.
    pub force: bool,
    /// Capture a snapshot before applying a profile.
    pub snapshot_before_write: bool,
}

impl Default for SafetyPolicy {
    fn default() -> Self {
        Self {
            allow_writes: true,
            allow_oc: false,
            dry_run: false,
            force: false,
            snapshot_before_write: true,
        }
    }
}

impl SafetyPolicy {
    pub fn require_writes(&self) -> Result<()> {
        if !self.allow_writes {
            return Err(Error::Safety(
                "writes are disabled (remove --read-only / enable them in the GUI)".into(),
            ));
        }
        Ok(())
    }

    pub fn require_oc(&self) -> Result<()> {
        self.require_writes()?;
        if !self.allow_oc {
            return Err(Error::Safety(
                "overclock writes require --allow-oc (or the OC switch in the GUI)".into(),
            ));
        }
        Ok(())
    }

    pub fn is_dry_run(&self) -> bool {
        self.dry_run
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Snapshot {
    pub version: u32,
    pub created: String,
    pub host: String,
    pub status: Status,
    pub wmi: BTreeMap<String, String>,
    pub hid: BTreeMap<String, String>,
    pub sysfs: BTreeMap<String, String>,
    pub profile: Profile,
}

impl Snapshot {
    pub fn new(status: Status, profile: Profile) -> Self {
        Self {
            version: 1,
            created: chrono::Local::now().to_rfc3339(),
            host: hostname(),
            status,
            wmi: BTreeMap::new(),
            hid: BTreeMap::new(),
            sysfs: sysfs_snapshot(),
            profile,
        }
    }
}

pub fn save_snapshot(snapshot: &Snapshot) -> Result<PathBuf> {
    config::ensure_dirs()?;
    let dir = config::snapshots_dir();
    fs::create_dir_all(&dir)?;
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
    let path = dir.join(format!("{stamp}.json"));
    fs::write(&path, serde_json::to_vec_pretty(snapshot)?)?;
    fs::write(
        dir.join("latest.json"),
        serde_json::to_vec_pretty(snapshot)?,
    )?;
    Ok(path)
}

pub fn latest_path() -> PathBuf {
    config::snapshots_dir().join("latest.json")
}

pub fn load_latest() -> Result<Snapshot> {
    let path = latest_path();
    if !path.exists() {
        return Err(Error::Config(format!(
            "no snapshot at {}; run `predator-sense snapshot` first",
            path.display()
        )));
    }
    load(&path)
}

pub fn load(path: &Path) -> Result<Snapshot> {
    let data = fs::read(path)?;
    Ok(serde_json::from_slice(&data)?)
}

pub fn list_snapshots() -> Result<Vec<PathBuf>> {
    let dir = config::snapshots_dir();
    let mut out = Vec::new();
    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.extension().and_then(|s| s.to_str()) == Some("json")
                && p.file_name().and_then(|s| s.to_str()) != Some("latest.json")
            {
                out.push(p);
            }
        }
    }
    out.sort();
    Ok(out)
}

pub fn sysfs_snapshot() -> BTreeMap<String, String> {
    let mut m = BTreeMap::new();
    let paths = [
        "/sys/devices/system/cpu/intel_pstate/no_turbo",
        "/sys/devices/system/cpu/cpufreq/policy0/scaling_governor",
        "/sys/devices/system/cpu/cpufreq/policy0/energy_performance_preference",
        "/sys/devices/system/cpu/cpufreq/policy0/scaling_max_freq",
        "/sys/class/powercap/intel-rapl:0/constraint_0_power_limit_uw",
        "/sys/class/powercap/intel-rapl:0/constraint_1_power_limit_uw",
        "/sys/class/powercap/intel-rapl:0/constraint_0_time_window_us",
        "/sys/class/powercap/intel-rapl:0/constraint_1_time_window_us",
        "/sys/class/power_supply/ACAD/online",
        "/sys/class/power_supply/BAT1/capacity",
        "/sys/class/power_supply/BAT1/status",
    ];
    for p in paths {
        if let Ok(v) = fs::read_to_string(p) {
            m.insert(p.to_string(), v.trim().to_string());
        }
    }
    m
}

fn hostname() -> String {
    fs::read_to_string("/proc/sys/kernel/hostname")
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| "unknown".to_string())
}

/// The safe, known-good state used by `panic` and the recovery guide.
pub fn safe_profile() -> Profile {
    let mut p = Profile {
        name: "safe".to_string(),
        ..Default::default()
    };
    p.lighting.mode = crate::lighting::LightingMode::Static;
    p.lighting.brightness = 50;
    p.lighting.color = crate::protocol::Rgb::DEFAULT_CYAN;
    p.lighting.keys.clear();
    p.ring.enabled = false;
    p.ring.brightness = 100;
    p.heatpipe = false;
    p.fan = Default::default();
    p.oc = Default::default();
    p
}

/// Everything off, fans auto, OC normal. Used by `panic`.
pub fn panic_profile() -> Profile {
    let mut p = safe_profile();
    p.name = "panic".to_string();
    p.lighting.mode = crate::lighting::LightingMode::Off;
    p.lighting.brightness = 0;
    p.ring.enabled = false;
    p.ring.brightness = 0;
    p.heatpipe = false;
    for f in [
        &mut p.fan.cpu,
        &mut p.fan.cpu2,
        &mut p.fan.gpu,
        &mut p.fan.gpu2,
        &mut p.fan.system,
    ] {
        f.auto = true;
    }
    p.oc = Default::default();
    p
}
