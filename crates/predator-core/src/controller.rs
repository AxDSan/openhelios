//! Ties the WMI and HID channels together and enforces the safety policy.

use std::fs;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::hid::HidKeyboard;
use crate::monitor::{Monitor, Sample};
use crate::oc;
use crate::profile::{FanKind, OcProfile, PowerProfile, Profile};
use crate::protocol::Rgb;
use crate::safety::{self, SafetyPolicy, Snapshot};
use crate::wmi::{hexs, AcerGaming};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Status {
    pub model: String,
    pub board: String,
    pub bios: String,
    pub kernel: String,
    pub wmi: bool,
    pub wmi_path: String,
    pub hid: bool,
    pub hid_path: Option<String>,
    pub hid_phys: Option<String>,
    pub hid_health: Option<String>,
    pub hid_b0: Option<String>,
    pub wmi_error: Option<String>,
    pub hid_error: Option<String>,
    pub nvidia_smi: bool,
}

pub struct Controller {
    pub policy: SafetyPolicy,
    pub wmi: Option<AcerGaming>,
    pub hid: Option<HidKeyboard>,
    pub status: Status,
    pub log: Vec<String>,
}

impl Controller {
    pub fn detect(policy: SafetyPolicy) -> Self {
        let mut status = Status {
            model: dmi("product_name").unwrap_or_default(),
            board: dmi("board_name").unwrap_or_default(),
            bios: dmi("bios_version").unwrap_or_default(),
            kernel: kernel(),
            wmi_path: crate::protocol::WMI_DEVICE.to_string(),
            nvidia_smi: which("nvidia-smi"),
            ..Default::default()
        };

        let wmi = match AcerGaming::open() {
            Ok(w) => {
                status.wmi = true;
                Some(w)
            }
            Err(e) => {
                status.wmi_error = Some(e.to_string());
                None
            }
        };

        let hid = match HidKeyboard::open() {
            Ok(k) => {
                status.hid = true;
                status.hid_path = Some(k.path().display().to_string());
                status.hid_phys = Some(k.phys().to_string());
                status.hid_health = Some(hexs(&k.health()));
                status.hid_b0 = k.status_b0().ok().map(|r| hexs(&r));
                Some(k)
            }
            Err(e) => {
                status.hid_error = Some(e.to_string());
                None
            }
        };

        let mut c = Self {
            policy,
            wmi,
            hid,
            status,
            log: Vec::new(),
        };
        c.note(format!(
            "detected {} / {}; WMI={} HID={}",
            c.status.model, c.status.board, c.status.wmi, c.status.hid
        ));
        c
    }

    pub fn note(&mut self, msg: impl Into<String>) {
        let msg = msg.into();
        let stamp = chrono::Local::now().format("%H:%M:%S");
        self.log.push(format!("{stamp} {msg}"));
        if self.log.len() > 500 {
            self.log.remove(0);
        }
    }

    pub fn sample(&mut self, monitor: &mut Monitor) -> Sample {
        monitor.sample()
    }

    pub fn has_any_device(&self) -> bool {
        self.wmi.is_some() || self.hid.is_some()
    }

    pub fn apply_profile(&mut self, profile: &Profile) -> Result<()> {
        self.policy.require_writes()?;
        let mut profile = profile.clone();
        profile.validate();

        if self.policy.is_dry_run() {
            self.note(format!(
                "dry-run: would apply profile {:?} (lighting {:?}, ring {}, heatpipe {}, OC {}/{})",
                profile.name,
                profile.lighting.mode,
                profile.ring.enabled,
                profile.heatpipe,
                profile.oc.cpu_level,
                profile.oc.gpu_level
            ));
            return Ok(());
        }

        if self.policy.snapshot_before_write {
            match self.save_state(&profile) {
                Ok(p) => self.note(format!("snapshot: {}", p.display())),
                Err(e) => self.note(format!("snapshot failed: {e}")),
            }
        }

        if self.hid.is_some() {
            self.apply_lighting(profile.lighting.state())?;
        } else {
            self.note("keyboard HID unavailable; lighting skipped");
        }
        if self.wmi.is_some() {
            self.apply_ring(
                profile.ring.enabled,
                profile.ring.brightness,
                profile.ring.color,
            )?;
            self.apply_heatpipe(profile.heatpipe)?;
            self.apply_fan_profile(&profile)?;
            self.apply_oc(&profile.oc)?;
            self.apply_power(&profile.power)?;
        } else {
            self.note("WMI unavailable; ring/heatpipe/fans/OC skipped");
        }
        Ok(())
    }

    pub fn apply_lighting(&mut self, state: crate::lighting::LightingState) -> Result<()> {
        self.policy.require_writes()?;
        if self.policy.is_dry_run() {
            self.note(format!(
                "dry-run: keyboard {:?} brightness {} effect {}",
                state.mode, state.brightness, state.effect
            ));
            return Ok(());
        }
        if let Some(hid) = &self.hid {
            state.apply(hid)?;
            self.note(format!(
                "keyboard: {:?} brightness={} effect={}",
                state.mode, state.brightness, state.effect
            ));
            Ok(())
        } else {
            Err(self
                .status
                .hid_error
                .clone()
                .map(Error::Hid)
                .unwrap_or(Error::HidUnavailable))
        }
    }

    pub fn apply_ring(&mut self, enabled: bool, brightness: u8, color: Rgb) -> Result<()> {
        self.policy.require_writes()?;
        if self.policy.is_dry_run() {
            self.note(format!(
                "dry-run: ring enabled={enabled} brightness={brightness}"
            ));
            return Ok(());
        }
        let Some(wmi) = &self.wmi else {
            return Err(self
                .status
                .wmi_error
                .clone()
                .map(Error::Wmi)
                .unwrap_or(Error::WmiUnavailable(crate::protocol::WMI_DEVICE.into())));
        };
        wmi.set_led_group("ring", enabled)?;
        let _ = wmi.set_led_group_color("ring", color, Some(brightness), enabled);
        let _ = wmi.set_ring_brightness(brightness);
        self.note(format!("ring: enabled={enabled} brightness={brightness}"));
        Ok(())
    }

    pub fn apply_heatpipe(&mut self, enabled: bool) -> Result<()> {
        self.policy.require_writes()?;
        if self.policy.is_dry_run() {
            self.note(format!("dry-run: heatpipe enabled={enabled}"));
            return Ok(());
        }
        let Some(wmi) = &self.wmi else {
            return Err(self
                .status
                .wmi_error
                .clone()
                .map(Error::Wmi)
                .unwrap_or(Error::WmiUnavailable(crate::protocol::WMI_DEVICE.into())));
        };
        wmi.set_led_group("heatpipe", enabled)?;
        self.note(format!("heatpipe: enabled={enabled}"));
        Ok(())
    }

    fn fan_kinds(&self) -> Vec<FanKind> {
        if self.status.board.contains("Levante") || self.status.model.contains("PH717") {
            vec![FanKind::Cpu, FanKind::Gpu]
        } else {
            FanKind::ALL.to_vec()
        }
    }

    fn apply_fan_profile(&mut self, profile: &Profile) -> Result<()> {
        let kinds = self.fan_kinds();
        for kind in kinds {
            let s = profile.fan.get(kind);
            self.apply_fan(kind, s.auto, s.percent)?;
        }
        Ok(())
    }

    pub fn apply_fan(&mut self, kind: FanKind, auto: bool, percent: u8) -> Result<()> {
        self.policy.require_writes()?;
        if self.policy.is_dry_run() {
            self.note(format!(
                "dry-run: fan {} auto={auto} percent={percent}",
                kind.label()
            ));
            return Ok(());
        }
        let Some(wmi) = &self.wmi else {
            return Err(self
                .status
                .wmi_error
                .clone()
                .map(Error::Wmi)
                .unwrap_or(Error::WmiUnavailable(crate::protocol::WMI_DEVICE.into())));
        };
        wmi.set_fan(kind, auto, percent)?;
        self.note(format!(
            "fan {}: {}",
            kind.label(),
            if auto {
                "auto".to_string()
            } else {
                format!("{percent}%")
            }
        ));
        Ok(())
    }

    pub fn apply_oc(&mut self, ocp: &OcProfile) -> Result<()> {
        if ocp.cpu_level == 0
            && ocp.gpu_level == 0
            && ocp.maxq_mode.is_none()
            && ocp.gpu_mode.is_none()
        {
            // Still write level 0 on an explicit apply so a previous profile is
            // cleared; this is cheap and reversible.
        }
        let enabling = ocp.cpu_level > 0
            || ocp.gpu_level > 0
            || ocp.maxq_mode.is_some()
            || ocp.gpu_mode.is_some();
        if enabling {
            self.policy.require_oc()?;
            let sample = Monitor::new().sample();
            oc::preflight(&sample)?;
        } else {
            // Resetting to firmware normal is always allowed.
            self.policy.require_writes()?;
        }

        if self.policy.is_dry_run() {
            self.note(format!(
                "dry-run: OC cpu={} gpu={} maxq={:?}/{:?} gpu_mode={:?}",
                ocp.cpu_level, ocp.gpu_level, ocp.maxq_mode, ocp.maxq_level, ocp.gpu_mode
            ));
            return Ok(());
        }

        let Some(wmi) = &self.wmi else {
            return Err(self
                .status
                .wmi_error
                .clone()
                .map(Error::Wmi)
                .unwrap_or(Error::WmiUnavailable(crate::protocol::WMI_DEVICE.into())));
        };

        wmi.set_cpu_oc(ocp.cpu_level)?;
        wmi.set_gpu_oc(ocp.gpu_level)?;
        if let (Some(mode), Some(level)) = (ocp.maxq_mode, ocp.maxq_level) {
            wmi.set_maxq(mode, level)?;
        }
        if let Some(mode) = ocp.gpu_mode {
            wmi.set_gpu_mode(mode)?;
        }
        if let Some(on) = ocp.post_sound {
            wmi.set_post_sound(on)?;
        }
        self.note(format!(
            "OC: cpu level {} gpu level {}",
            ocp.cpu_level, ocp.gpu_level
        ));
        Ok(())
    }

    pub fn apply_power(&mut self, p: &PowerProfile) -> Result<()> {
        if p.pl1_w.is_none()
            && p.pl2_w.is_none()
            && p.gpu_power_w.is_none()
            && p.gpu_lock_min_mhz.is_none()
            && p.gpu_lock_max_mhz.is_none()
        {
            return Ok(());
        }
        self.policy.require_oc()?;
        if self.policy.is_dry_run() {
            self.note(format!("dry-run: power settings {p:?}"));
            return Ok(());
        }
        if let Some(w) = p.pl1_w {
            oc::PowerLimits::read().set_pl1(w, None)?;
        }
        if let Some(w) = p.pl2_w {
            oc::PowerLimits::read().set_pl2(w, None)?;
        }
        if let Some(w) = p.gpu_power_w {
            oc::set_nvidia_power_limit(w)?;
        }
        if let (Some(min), Some(max)) = (p.gpu_lock_min_mhz, p.gpu_lock_max_mhz) {
            oc::set_gpu_locked_clocks(min, max)?;
        }
        self.note("power/GPU limits applied");
        Ok(())
    }

    pub fn panic(&mut self) -> Result<()> {
        let p = safety::panic_profile();
        if self.policy.is_dry_run() {
            self.note("dry-run: panic reset (nothing written)");
            return Ok(());
        }

        // Panic is allowed even when normal writes are disabled.
        let mut msgs: Vec<String> = Vec::new();
        if let Some(hid) = &self.hid {
            match hid.off() {
                Ok(()) => msgs.push("panic: keyboard off".to_string()),
                Err(e) => msgs.push(format!("panic: keyboard off failed: {e}")),
            }
        }
        if let Some(wmi) = self.wmi.clone() {
            for group in ["ring", "heatpipe", "cpufan", "gpufan"] {
                if let Err(e) = wmi.set_led_group(group, false) {
                    msgs.push(format!("panic: {group} off failed: {e}"));
                }
            }
            let _ = wmi.set_ring_brightness(0);
            let _ = wmi.set_cpu_oc(0);
            let _ = wmi.set_gpu_oc(0);
            for kind in [FanKind::Cpu, FanKind::Gpu] {
                if let Err(e) = wmi.set_fan(kind, true, 0) {
                    msgs.push(format!("panic: fan {} auto failed: {e}", kind.label()));
                }
            }
            msgs.push("panic: OC normal, fans auto, lights off".to_string());
        }
        for m in msgs {
            self.note(m);
        }
        let _ = p;
        Ok(())
    }

    pub fn snapshot(&self, profile: &Profile) -> Snapshot {
        let mut snap = Snapshot::new(self.status.clone(), profile.clone());
        if let Some(wmi) = &self.wmi {
            let mut put = |key: &str, r: Result<Vec<u8>>| {
                let value = match r {
                    Ok(v) => hexs(&v),
                    Err(e) => format!("ERR {e}"),
                };
                snap.wmi.insert(key.to_string(), value);
            };
            put("sysinfo0", wmi.get_sysinfo(0));
            put("sysinfo1", wmi.get_sysinfo(1));
            put("sysinfo2", wmi.get_sysinfo(2));
            put("gaming_led", wmi.get_gaming_led());
            put("kbbl", wmi.get_kbbl());
            for (key, group) in [
                ("group_cpufan", "cpufan"),
                ("group_gpufan", "gpufan"),
                ("group_ring", "ring"),
                ("group_light1", "light1"),
                ("group_light", "light"),
            ] {
                let v = wmi
                    .get_led_group(group)
                    .map(protocol_u64_le)
                    .map(|b| b.to_vec());
                put(key, v);
            }
            for sel in [2u64, 3, 6, 9, 270] {
                let v = wmi.get_misc(sel).map(protocol_u64_le).map(|b| b.to_vec());
                put(&format!("misc{sel}"), v);
            }
            for (key, group) in [("fan_cpu", FanKind::Cpu), ("fan_gpu", FanKind::Gpu)] {
                let v = wmi.get_fan_behavior(match group {
                    FanKind::Cpu => 1,
                    FanKind::Gpu => 4,
                    _ => 0,
                });
                put(key, v.map(protocol_u64_le).map(|b| b.to_vec()));
            }
        }
        if let Some(hid) = &self.hid {
            snap.hid.insert("health".into(), hexs(&hid.health()));
            if let Ok(b) = hid.status_b0() {
                snap.hid.insert("status_b0".into(), hexs(&b));
            }
            if let Ok(s) = hid.status() {
                snap.hid.insert("status_88".into(), hexs(&s));
            }
        }
        snap
    }

    pub fn save_state(&self, profile: &Profile) -> Result<std::path::PathBuf> {
        let snap = self.snapshot(profile);
        safety::save_snapshot(&snap)
    }

    pub fn restore_latest(&mut self, apply_profile: bool) -> Result<()> {
        let snap = safety::load_latest()?;
        self.note(format!("restoring snapshot {}", snap.created));
        if apply_profile {
            self.apply_profile(&snap.profile)
        } else {
            self.panic()
        }
    }
}

fn protocol_u64_le(v: u64) -> [u8; 8] {
    v.to_le_bytes()
}

fn dmi(field: &str) -> Option<String> {
    fs::read_to_string(format!("/sys/class/dmi/id/{field}"))
        .ok()
        .map(|s| s.trim().to_string())
}

fn kernel() -> String {
    fs::read_to_string("/proc/sys/kernel/osrelease")
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

fn which(name: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|p| p.join(name).is_file()))
        .unwrap_or(false)
}
