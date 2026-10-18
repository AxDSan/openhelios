//! Capture/apply/restore for the keyboard-slide Turbo state.
//!
//! The Windows app bumps the CPU/GPU OC profile by two steps when the slide
//! switch is engaged. On Linux the equivalent is a performance CPU policy plus
//! RAPL/NVIDIA limits; the visual/fan part lives in `Controller`.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::config;
use crate::error::Result;
use crate::oc;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TurboState {
    pub governor: Option<String>,
    pub epp: Option<String>,
    pub no_turbo: Option<bool>,
    pub max_freq_khz: Option<u64>,
    pub pl1_w: Option<f32>,
    pub pl2_w: Option<f32>,
    pub gpu_power_w: Option<f32>,
    pub gpu_lock_min_mhz: Option<f32>,
    pub gpu_lock_max_mhz: Option<f32>,
}

impl TurboState {
    pub fn capture() -> Self {
        let cpu = oc::cpu_status();
        let limits = oc::PowerLimits::read();
        let nv = oc::nvidia_status();
        Self {
            governor: cpu.governor,
            epp: cpu.epp,
            no_turbo: cpu.no_turbo,
            max_freq_khz: cpu.max_freq_khz,
            pl1_w: limits.pl1_uw.map(|v| v as f32 / 1_000_000.0),
            pl2_w: limits.pl2_uw.map(|v| v as f32 / 1_000_000.0),
            gpu_power_w: nv.as_ref().and_then(|n| n.power_limit_w),
            gpu_lock_min_mhz: None,
            gpu_lock_max_mhz: None,
        }
    }

    pub fn path() -> PathBuf {
        config::state_dir().join("turbo-state.json")
    }

    pub fn save(&self) -> Result<PathBuf> {
        config::ensure_dirs()?;
        let path = Self::path();
        std::fs::write(&path, serde_json::to_vec_pretty(self)?)?;
        Ok(path)
    }

    pub fn load() -> Result<Self> {
        let path = Self::path();
        let data = std::fs::read(&path)?;
        Ok(serde_json::from_slice(&data)?)
    }

    /// Apply the Turbo CPU/GPU limits. Requires root for RAPL/cpufreq/NVIDIA.
    pub fn apply(&self) -> Result<()> {
        let cpu = oc::cpu_status();
        oc::set_governor("performance")?;
        if !cpu.available_epp.is_empty() {
            let _ = oc::set_epp("performance");
        }
        oc::set_no_turbo(false)?;
        if let Some(freq) = cpu.max_freq_khz {
            oc::set_max_freq_khz(freq)?;
        }

        let limits = oc::PowerLimits::read();
        limits.set_pl1(55.0, None)?;
        limits.set_pl2(70.0, None)?;

        if let Some(nv) = oc::nvidia_status() {
            if let Some(max_power) = nv.max_power_w {
                oc::set_nvidia_power_limit(max_power)?;
            }
            if let Some(max_clock) = nv.max_clock_mhz {
                oc::set_gpu_locked_clocks(300.0, max_clock)?;
            }
        }
        Ok(())
    }

    /// Restore the pre-Turbo state. Requires root for RAPL/cpufreq/NVIDIA.
    pub fn restore(&self) -> Result<()> {
        if let Some(governor) = &self.governor {
            oc::set_governor(governor)?;
        }
        if let Some(epp) = &self.epp {
            let _ = oc::set_epp(epp);
        }
        if let Some(no_turbo) = self.no_turbo {
            oc::set_no_turbo(no_turbo)?;
        }
        if let Some(freq) = self.max_freq_khz {
            oc::set_max_freq_khz(freq)?;
        }
        if let Some(watts) = self.pl1_w {
            oc::PowerLimits::read().set_pl1(watts, None)?;
        }
        if let Some(watts) = self.pl2_w {
            oc::PowerLimits::read().set_pl2(watts, None)?;
        }
        if let Some(watts) = self.gpu_power_w {
            oc::set_nvidia_power_limit(watts)?;
        }
        if let (Some(min), Some(max)) = (self.gpu_lock_min_mhz, self.gpu_lock_max_mhz) {
            oc::set_gpu_locked_clocks(min, max)?;
        } else {
            let _ = oc::reset_gpu_clocks();
        }
        Ok(())
    }
}
