//! Firmware overclock levels plus the Linux-side CPU controls.
//!
//! PredatorSense on Windows performs CPU OC through Intel's XTU SDK and GPU OC
//! through NVAPI. The EC also exposes three firmware OC levels over WMI; those
//! are implemented here and are the safest cross-OS path. Linux CPU governor,
//! EPP, turbo and frequency limits are exposed as reversible equivalents.
//! MSR writes are deliberately not performed by default.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::monitor::Sample;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OcLevel {
    Normal,
    Fast,
    Turbo,
}

impl OcLevel {
    pub fn level(self) -> u8 {
        match self {
            OcLevel::Normal => 0,
            OcLevel::Fast => 1,
            OcLevel::Turbo => 2,
        }
    }

    pub fn from_level(v: u8) -> Option<Self> {
        match v {
            0 => Some(OcLevel::Normal),
            1 => Some(OcLevel::Fast),
            2 => Some(OcLevel::Turbo),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            OcLevel::Normal => "Normal",
            OcLevel::Fast => "Fast",
            OcLevel::Turbo => "Turbo",
        }
    }

    pub const ALL: [OcLevel; 3] = [OcLevel::Normal, OcLevel::Fast, OcLevel::Turbo];
}

/// Acer's own rule: AC connected and at least 40% battery.
pub fn preflight(sample: &Sample) -> Result<()> {
    if sample.ac_online == Some(false) {
        return Err(Error::Safety(
            "overclocking requires the AC adapter to be connected".into(),
        ));
    }
    if let Some(pct) = sample.battery_percent {
        if pct < 40.0 {
            return Err(Error::Safety(format!(
                "battery is {pct:.0}%; charge to at least 40% before overclocking"
            )));
        }
    }
    Ok(())
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CpuStatus {
    pub governor: Option<String>,
    pub available_governors: Vec<String>,
    pub epp: Option<String>,
    pub available_epp: Vec<String>,
    pub no_turbo: Option<bool>,
    pub max_freq_khz: Option<u64>,
    pub cur_freq_khz: Option<u64>,
    pub driver: Option<String>,
    pub msr_available: bool,
}

pub fn cpu_status() -> CpuStatus {
    let policy_path = [
        PathBuf::from("/sys/devices/system/cpu/cpufreq/policy0"),
        PathBuf::from("/sys/devices/system/cpu/cpu0/cpufreq"),
    ]
    .into_iter()
    .find(|p| p.exists())
    .unwrap_or_else(|| PathBuf::from("/sys/devices/system/cpu/cpufreq/policy0"));
    let policy = policy_path.as_path();
    CpuStatus {
        governor: read_string(&policy.join("scaling_governor")),
        available_governors: read_list(&policy.join("scaling_available_governors")),
        epp: read_string(&policy.join("energy_performance_preference")),
        available_epp: read_list(&policy.join("energy_performance_available_preferences")),
        no_turbo: read_string(&PathBuf::from(
            "/sys/devices/system/cpu/intel_pstate/no_turbo",
        ))
        .map(|v| v == "1"),
        max_freq_khz: read_u64(&policy.join("scaling_max_freq")),
        cur_freq_khz: read_u64(&policy.join("scaling_cur_freq")),
        driver: read_string(&policy.join("scaling_driver")),
        msr_available: Path::new("/dev/cpu/0/msr").exists(),
    }
}

pub fn set_governor(governor: &str) -> Result<usize> {
    write_all_cpu("scaling_governor", governor.as_bytes())
}

pub fn set_epp(epp: &str) -> Result<usize> {
    write_all_cpu("energy_performance_preference", epp.as_bytes())
}

pub fn set_max_freq_khz(khz: u64) -> Result<usize> {
    write_all_cpu("scaling_max_freq", khz.to_string().as_bytes())
}

pub fn set_no_turbo(enabled: bool) -> Result<()> {
    let path = Path::new("/sys/devices/system/cpu/intel_pstate/no_turbo");
    if !path.exists() {
        return Err(Error::Unsupported(
            "intel_pstate no_turbo is not available".into(),
        ));
    }
    fs::write(path, if enabled { b"1" } else { b"0" })?;
    Ok(())
}

fn write_all_cpu(file: &str, value: &[u8]) -> Result<usize> {
    let mut n = 0;
    for dir in cpu_dirs() {
        let p = dir.join(file);
        if p.exists() {
            fs::write(&p, value).map_err(|e| {
                Error::Io(std::io::Error::new(
                    e.kind(),
                    format!("{}: {e}", p.display()),
                ))
            })?;
            n += 1;
        }
    }
    if n == 0 {
        return Err(Error::Unsupported(format!("no CPU exposes {file}")));
    }
    Ok(n)
}

fn cpu_dirs() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(entries) = fs::read_dir("/sys/devices/system/cpu") {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with("cpu") && name[3..].chars().all(|c| c.is_ascii_digit()) {
                let base = entry.path();
                let policy = base.join("cpufreq");
                if policy.exists() {
                    out.push(policy);
                }
                if let Ok(pols) = fs::read_dir(base.join("cpufreq")) {
                    for p in pols.flatten() {
                        if p.file_name().to_string_lossy().starts_with("policy") {
                            out.push(p.path());
                        }
                    }
                }
            }
        }
    }
    out
}

fn read_string(path: &Path) -> Option<String> {
    fs::read_to_string(path).ok().map(|s| s.trim().to_string())
}

fn read_list(path: &Path) -> Vec<String> {
    read_string(path)
        .map(|s| s.split_whitespace().map(str::to_string).collect())
        .unwrap_or_default()
}

fn read_u64(path: &Path) -> Option<u64> {
    read_string(path)?.parse().ok()
}

/// RAPL package power limits (PL1/PL2) exposed by the powercap sysfs.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PowerLimits {
    pub pl1_uw: Option<u64>,
    pub pl1_window_us: Option<u64>,
    pub pl2_uw: Option<u64>,
    pub pl2_window_us: Option<u64>,
}

fn rapl(name: &str) -> PathBuf {
    PathBuf::from("/sys/class/powercap/intel-rapl:0").join(name)
}

impl PowerLimits {
    pub fn read() -> Self {
        Self {
            pl1_uw: read_u64(&rapl("constraint_0_power_limit_uw")),
            pl1_window_us: read_u64(&rapl("constraint_0_time_window_us")),
            pl2_uw: read_u64(&rapl("constraint_1_power_limit_uw")),
            pl2_window_us: read_u64(&rapl("constraint_1_time_window_us")),
        }
    }

    pub fn set_pl1(&self, watts: f32, window_ms: Option<u64>) -> Result<()> {
        let uw = watts_to_uw(watts)?;
        write_number(&rapl("constraint_0_power_limit_uw"), uw)?;
        if let Some(ms) = window_ms {
            write_number(
                &rapl("constraint_0_time_window_us"),
                ms.saturating_mul(1000),
            )?;
        }
        Ok(())
    }

    pub fn set_pl2(&self, watts: f32, window_ms: Option<u64>) -> Result<()> {
        let uw = watts_to_uw(watts)?;
        write_number(&rapl("constraint_1_power_limit_uw"), uw)?;
        if let Some(ms) = window_ms {
            write_number(
                &rapl("constraint_1_time_window_us"),
                ms.saturating_mul(1000),
            )?;
        }
        Ok(())
    }
}

fn watts_to_uw(watts: f32) -> Result<u64> {
    if !(5.0..=200.0).contains(&watts) {
        return Err(Error::Safety(format!(
            "power limit {watts} W is outside the supported 5..200 W range"
        )));
    }
    Ok((watts * 1_000_000.0).round() as u64)
}

fn write_number(path: &Path, value: u64) -> Result<()> {
    fs::write(path, value.to_string()).map_err(|e| {
        Error::Io(std::io::Error::new(
            e.kind(),
            format!("{}: {e}", path.display()),
        ))
    })
}

/// NVML/NVIDIA driver view used by the Windows app's NVAPI path.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct NvidiaPower {
    pub power_limit_w: Option<f32>,
    pub min_power_w: Option<f32>,
    pub max_power_w: Option<f32>,
    pub max_clock_mhz: Option<f32>,
    pub current_clock_mhz: Option<f32>,
}

pub fn nvidia_status() -> Option<NvidiaPower> {
    let out = std::process::Command::new("nvidia-smi")
        .args([
            "--query-gpu=power.limit,power.min_limit,power.max_limit,clocks.max.graphics,clocks.current.graphics",
            "--format=csv,noheader,nounits",
        ])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let line = text.lines().next()?;
    let f: Vec<&str> = line.split(',').map(str::trim).collect();
    let n = |i: usize| f.get(i).and_then(|v| v.parse::<f32>().ok());
    Some(NvidiaPower {
        power_limit_w: n(0),
        min_power_w: n(1),
        max_power_w: n(2),
        max_clock_mhz: n(3),
        current_clock_mhz: n(4),
    })
}

pub fn set_nvidia_power_limit(watts: f32) -> Result<()> {
    if !(5.0..=500.0).contains(&watts) {
        return Err(Error::Safety(format!(
            "GPU power limit {watts} W is out of range"
        )));
    }
    run_nvidia(&["-pl", &format!("{watts:.1}")])
}

pub fn set_gpu_locked_clocks(min_mhz: f32, max_mhz: f32) -> Result<()> {
    if min_mhz < 0.0 || max_mhz < min_mhz || max_mhz > 4000.0 {
        return Err(Error::Safety(format!(
            "invalid GPU clock lock {min_mhz}..{max_mhz} MHz"
        )));
    }
    run_nvidia(&["-lgc", &format!("{min_mhz:.0},{max_mhz:.0}")])
}

pub fn reset_gpu_clocks() -> Result<()> {
    run_nvidia(&["-rgc"])
}

fn run_nvidia(args: &[&str]) -> Result<()> {
    let out = std::process::Command::new("nvidia-smi")
        .args(args)
        .output()
        .map_err(|e| Error::Io(std::io::Error::new(e.kind(), format!("nvidia-smi: {e}"))))?;
    if !out.status.success() {
        let msg = String::from_utf8_lossy(&out.stderr);
        return Err(Error::Unsupported(format!(
            "nvidia-smi {} failed: {}",
            args.join(" "),
            msg.trim()
        )));
    }
    Ok(())
}
