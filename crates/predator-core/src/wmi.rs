//! Userspace side of `acer_gaming_probe.ko`.
//!
//! `write()` is `[method_id] + payload`; `read()` returns the raw response.

use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::protocol::{self, method, Rgb};

#[derive(Debug, Clone)]
pub struct Wmi {
    path: PathBuf,
}

impl Wmi {
    pub fn open() -> Result<Self> {
        Self::open_path(protocol::WMI_DEVICE)
    }

    pub fn open_path(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        if !path.exists() {
            return Err(Error::WmiUnavailable(path));
        }
        Ok(Self { path })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn call(&self, method_id: u8, payload: &[u8]) -> Result<Vec<u8>> {
        let mut f = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&self.path)
            .map_err(|e| match e.kind() {
                std::io::ErrorKind::PermissionDenied => Error::Permission {
                    path: self.path.clone(),
                },
                _ => Error::Io(e),
            })?;

        let mut req = Vec::with_capacity(payload.len() + 1);
        req.push(method_id);
        req.extend_from_slice(payload);
        f.write_all(&req)?;

        let mut resp = Vec::new();
        f.read_to_end(&mut resp)?;
        Ok(resp)
    }

    pub fn call_u64(&self, method_id: u8, value: u64) -> Result<Vec<u8>> {
        self.call(method_id, &protocol::u64le(value))
    }
}

/// High-level view of the Acer gaming-function mailbox.
#[derive(Debug, Clone)]
pub struct AcerGaming {
    pub wmi: Wmi,
}

impl AcerGaming {
    pub fn open() -> Result<Self> {
        Ok(Self { wmi: Wmi::open()? })
    }

    pub fn raw(&self, method_id: u8, payload: &[u8]) -> Result<Vec<u8>> {
        self.wmi.call(method_id, payload)
    }

    pub fn get_sysinfo(&self, selector: u8) -> Result<Vec<u8>> {
        self.wmi.call_u64(method::GET_SYS_INFO, selector as u64)
    }

    /// `GetGamingSysInfo(1 | index<<8)` health query used by PredatorSense.
    /// Returns `None` when the firmware reports a non-zero status byte.
    pub fn get_health(&self, index: u8) -> Result<Option<u32>> {
        let payload = 1u64 | ((index as u64) << 8);
        let r = self.wmi.call_u64(method::GET_SYS_INFO, payload)?;
        let v = protocol::le_u64(&r);
        if v & 0xFF == 0 {
            Ok(Some(((v >> 8) & 0xFFFF) as u32))
        } else {
            Ok(None)
        }
    }

    pub fn get_gaming_led(&self) -> Result<Vec<u8>> {
        self.wmi.call(method::GET_GAMING_LED, &[])
    }

    pub fn set_gaming_led(&self, value: u64) -> Result<Vec<u8>> {
        self.wmi.call_u64(method::SET_GAMING_LED, value)
    }

    pub fn set_static_zone(&self, zone: u8, rgb: Rgb) -> Result<Vec<u8>> {
        self.wmi.call(
            method::SET_STATIC_LED,
            &protocol::static_zone_payload(zone, rgb),
        )
    }

    pub fn set_led_group(&self, group: &str, on: bool) -> Result<Vec<u8>> {
        let value = protocol::led_group_payload(group, on)
            .ok_or_else(|| Error::Unsupported(format!("LED group {group}")))?;
        self.wmi.call_u64(method::SET_LED_GROUP_BEHAVIOR, value)
    }

    pub fn get_led_group(&self, group: &str) -> Result<u64> {
        let payload = protocol::led_group_payload(group, false)
            .ok_or_else(|| Error::Unsupported(format!("LED group {group}")))?;
        let r = self.wmi.call_u64(method::GET_LED_GROUP_BEHAVIOR, payload)?;
        Ok(protocol::le_u64(&r))
    }

    pub fn set_led_group_color(
        &self,
        group: &str,
        rgb: Rgb,
        brightness: Option<u8>,
        enable: bool,
    ) -> Result<Vec<u8>> {
        let value = protocol::led_color_payload(group, rgb, brightness, enable)
            .ok_or_else(|| Error::Unsupported(format!("LED group {group}")))?;
        self.wmi.call_u64(method::SET_LED_GROUP_COLOR, value)
    }

    pub fn set_kbbl(&self, packet: &[u8; 16]) -> Result<Vec<u8>> {
        self.wmi.call(method::SET_KBBL, packet)
    }

    /// Touchpad-ring brightness. PredatorSense sends method 20 in its u64
    /// form: `WMISetGamingKBBacklight(0 | (brightness << 16))`.
    pub fn set_ring_brightness(&self, brightness: u8) -> Result<Vec<u8>> {
        self.wmi
            .call_u64(method::SET_KBBL, (brightness.min(100) as u64) << 16)
    }

    pub fn get_kbbl(&self) -> Result<Vec<u8>> {
        self.wmi.call(method::GET_KBBL, &[])
    }

    pub fn set_misc(&self, value: u64) -> Result<Vec<u8>> {
        self.wmi.call_u64(method::SET_MISC, value)
    }

    pub fn get_misc(&self, selector: u64) -> Result<u64> {
        let r = self.wmi.call_u64(method::GET_MISC, selector)?;
        Ok(protocol::le_u64(&r))
    }

    pub fn set_fan_behavior(&self, value: u64) -> Result<Vec<u8>> {
        self.wmi.call_u64(method::SET_FAN_BEHAVIOR, value)
    }

    pub fn get_fan_behavior(&self, selector: u64) -> Result<u64> {
        let r = self.wmi.call_u64(method::GET_FAN_BEHAVIOR, selector)?;
        Ok(protocol::le_u64(&r))
    }

    pub fn set_fan_speed(&self, value: u64) -> Result<Vec<u8>> {
        self.wmi.call_u64(method::SET_FAN_SPEED, value)
    }

    pub fn get_fan_speed(&self, selector: u64) -> Result<u64> {
        let r = self.wmi.call_u64(method::GET_FAN_SPEED, selector)?;
        Ok(protocol::le_u64(&r))
    }

    pub fn set_function(&self, value: u64) -> Result<Vec<u8>> {
        self.wmi.call_u64(method::SET_FUNCTION, value)
    }

    pub fn get_function(&self, selector: u64) -> Result<u64> {
        let r = self.wmi.call_u64(method::GET_FUNCTION, selector)?;
        Ok(protocol::le_u64(&r))
    }

    /// Set a firmware overclock level (0 normal, 1 fast, 2 turbo).
    pub fn set_cpu_oc(&self, level: u8) -> Result<Vec<u8>> {
        let v = protocol::oc_cpu_payload(level)
            .ok_or_else(|| Error::Unsupported(format!("CPU OC level {level}")))?;
        self.set_misc(v)
    }

    pub fn set_gpu_oc(&self, level: u8) -> Result<Vec<u8>> {
        let v = protocol::oc_gpu_payload(level)
            .ok_or_else(|| Error::Unsupported(format!("GPU OC level {level}")))?;
        self.set_misc(v)
    }

    pub fn set_maxq(&self, mode: u8, level: u8) -> Result<Vec<u8>> {
        let v = protocol::maxq_payload(mode, level)
            .ok_or_else(|| Error::Unsupported(format!("Max-Q mode {mode} level {level}")))?;
        self.set_misc(v)
    }

    pub fn set_gpu_mode(&self, mode: u8) -> Result<Vec<u8>> {
        self.set_misc(protocol::gpu_mode_payload(mode))
    }

    pub fn set_post_sound(&self, on: bool) -> Result<Vec<u8>> {
        self.set_misc(protocol::post_sound_payload(on))
    }

    /// Per-fan manual/auto control, matching PredatorSense's payloads.
    pub fn set_fan(&self, group: crate::profile::FanKind, auto: bool, percent: u8) -> Result<()> {
        use crate::profile::FanKind;
        let (behavior, speed_group) = match (group, auto) {
            (FanKind::Cpu, true) => (protocol::fan::CPU_AUTO, protocol::fan::CPU),
            (FanKind::Cpu, false) => (protocol::fan::CPU_MANUAL, protocol::fan::CPU),
            (FanKind::Cpu2, true) => (protocol::fan::SYS2_AUTO, protocol::fan::SYS2),
            (FanKind::Cpu2, false) => (protocol::fan::SYS2_MANUAL, protocol::fan::SYS2),
            (FanKind::Gpu, true) => (protocol::fan::GPU1_AUTO, protocol::fan::GPU1),
            (FanKind::Gpu, false) => (protocol::fan::GPU1_MANUAL, protocol::fan::GPU1),
            (FanKind::Gpu2, true) => (protocol::fan::GPU2_AUTO, protocol::fan::GPU2),
            (FanKind::Gpu2, false) => (protocol::fan::GPU2_MANUAL, protocol::fan::GPU2),
            (FanKind::System, true) => (protocol::fan::SYS1_AUTO, protocol::fan::SYS1),
            (FanKind::System, false) => (protocol::fan::SYS1_MANUAL, protocol::fan::SYS1),
        };
        self.set_fan_behavior(behavior)?;
        if !auto {
            let v = (speed_group as u64) | (((percent.min(100)) as u64) << 8);
            self.set_fan_speed(v)?;
        }
        Ok(())
    }
}

pub fn hexs(data: &[u8]) -> String {
    if data.is_empty() {
        return "(empty)".to_string();
    }
    data.iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
}
