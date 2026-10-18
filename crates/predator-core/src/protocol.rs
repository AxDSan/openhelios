//! Wire formats recovered from PredatorSense 3.00.3196 and 3.00.3152.
//!
//! All WMI payloads are little-endian u64 values unless noted otherwise. The
//! method ids are `Arg1` of the ACPI `WMBH` dispatcher.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub const WMI_DEVICE: &str = "/dev/acer_gaming_wmi";

pub mod method {
    pub const SET_GAMING_LED: u8 = 2;
    pub const GET_GAMING_LED: u8 = 4;
    pub const GET_SYS_INFO: u8 = 5;
    pub const SET_STATIC_LED: u8 = 6;
    pub const SET_LED_GROUP_BEHAVIOR: u8 = 10;
    pub const GET_LED_GROUP_BEHAVIOR: u8 = 11;
    pub const SET_LED_GROUP_COLOR: u8 = 12;
    pub const GET_LED_GROUP_COLOR: u8 = 13;
    pub const SET_FAN_BEHAVIOR: u8 = 14;
    pub const GET_FAN_BEHAVIOR: u8 = 15;
    pub const SET_FAN_SPEED: u8 = 16;
    pub const GET_FAN_SPEED: u8 = 17;
    pub const SET_KBBL: u8 = 20;
    pub const GET_KBBL: u8 = 21;
    pub const SET_MISC: u8 = 22;
    pub const GET_MISC: u8 = 23;
    /// `WMISetFunction` in PredatorSense; used for CoolBoost.
    pub const SET_FUNCTION: u8 = 0;
    /// `WMIGetFunction`.
    pub const GET_FUNCTION: u8 = 1;
}

/// LED group ids used by method 10 (`SetGamingLEDGroupBehavior`).
pub mod led_group {
    pub const CPU_FAN: u64 = 0x20;
    pub const GPU_FAN: u64 = 0x40;
    pub const TOUCHPAD_RING: u64 = 0x80;
    pub const LIGHT_1: u64 = 0x100;
    pub const LIGHT_OTHER: u64 = 0x01;
}

/// Colour indices used by method 12 (`SetGamingLEDGroupColor`).
pub mod led_color_index {
    pub const CPU_FAN: u64 = 6;
    pub const GPU_FAN: u64 = 7;
    pub const TOUCHPAD_RING: u64 = 8;
    pub const LIGHT_1: u64 = 9;
    pub const LIGHT_OTHER: u64 = 1;
}

/// Fan groups used by methods 14/16.
pub mod fan {
    pub const CPU: u8 = 1;
    pub const SYS1: u8 = 2;
    pub const SYS2: u8 = 3;
    pub const GPU1: u8 = 4;
    pub const GPU2: u8 = 5;

    // Behaviour payloads from PredatorSense `set_single_custom_fan_state`.
    pub const CPU_AUTO: u64 = 0x1_0001;
    pub const CPU_MANUAL: u64 = 0x3_0001;
    pub const SYS1_AUTO: u64 = 0x4_0002;
    pub const SYS1_MANUAL: u64 = 0xC_0002;
    pub const SYS2_AUTO: u64 = 0x1_0004;
    pub const SYS2_MANUAL: u64 = 0x3_0004;
    pub const GPU1_AUTO: u64 = 0x4_0008;
    pub const GPU1_MANUAL: u64 = 0xC_0008;
    pub const GPU2_AUTO: u64 = 0x1_0010;
    pub const GPU2_MANUAL: u64 = 0x3_0010;
}

/// Firmware overclock levels from `CommonFunction.Set_OC_level`.
pub fn oc_cpu_payload(level: u8) -> Option<u64> {
    match level {
        0 => Some(7),
        1 => Some(263),
        2 => Some(519),
        _ => None,
    }
}

pub fn oc_gpu_payload(level: u8) -> Option<u64> {
    match level {
        0 => Some(5),
        1 => Some(261),
        2 => Some(517),
        _ => None,
    }
}

/// Max-Q mode/level from `CommonFunction.Set_MaxQStatus`.
pub fn maxq_payload(mode: u8, level: u8) -> Option<u64> {
    match (mode, level) {
        (1, 0) => Some(4101),
        (1, 1) => Some(4357),
        (1, 2) => Some(4613),
        (2, 0) => Some(8197),
        (2, 1) => Some(8453),
        (2, 2) => Some(8709),
        _ => None,
    }
}

pub fn gpu_mode_payload(mode: u8) -> u64 {
    2u64 | ((mode as u64) << 8)
}

pub fn post_sound_payload(on: bool) -> u64 {
    6u64 | ((on as u64) << 8)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const WHITE: Rgb = Rgb {
        r: 255,
        g: 255,
        b: 255,
    };
    pub const BLACK: Rgb = Rgb { r: 0, g: 0, b: 0 };
    pub const DEFAULT_CYAN: Rgb = Rgb {
        r: 0,
        g: 174,
        b: 199,
    };

    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    pub fn hex(self) -> String {
        format!("#{:02X}{:02X}{:02X}", self.r, self.g, self.b)
    }

    pub fn from_hex(s: &str) -> Option<Self> {
        let s = s.trim().trim_start_matches('#');
        if s.len() != 6 {
            return None;
        }
        let r = u8::from_str_radix(&s[0..2], 16).ok()?;
        let g = u8::from_str_radix(&s[2..4], 16).ok()?;
        let b = u8::from_str_radix(&s[4..6], 16).ok()?;
        Some(Self { r, g, b })
    }

    pub fn to_array(self) -> [u8; 3] {
        [self.r, self.g, self.b]
    }

    pub fn from_array(a: [u8; 3]) -> Self {
        Self {
            r: a[0],
            g: a[1],
            b: a[2],
        }
    }
}

impl Serialize for Rgb {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.hex())
    }
}

impl<'de> Deserialize<'de> for Rgb {
    fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Rgb::from_hex(&s)
            .ok_or_else(|| serde::de::Error::custom(format!("expected #RRGGBB, got {s:?}")))
    }
}

pub fn u64le(v: u64) -> [u8; 8] {
    v.to_le_bytes()
}

pub fn le_u64(data: &[u8]) -> u64 {
    let mut b = [0u8; 8];
    let n = data.len().min(8);
    b[..n].copy_from_slice(&data[..n]);
    u64::from_le_bytes(b)
}

/// 16-byte keyboard-backlight effect struct (method 20).
pub fn kbbl_packet(
    mode: u8,
    speed: u8,
    brightness: u8,
    direction: u8,
    rgb: Rgb,
    enable: u8,
) -> [u8; 16] {
    let mut p = [0u8; 16];
    p[0] = mode;
    p[1] = speed;
    p[2] = brightness;
    p[3] = if mode == 3 { 8 } else { 0 };
    p[4] = direction;
    p[5] = rgb.r;
    p[6] = rgb.g;
    p[7] = rgb.b;
    p[9] = enable;
    p
}

pub fn led_group_payload(group: &str, on: bool) -> Option<u64> {
    let (id, bit) = match group {
        "cpufan" | "cpu-fan" | "cpu" => (led_group::CPU_FAN, 26),
        "gpufan" | "gpu-fan" | "gpu" | "heatpipe" | "heat-pipe" => (led_group::GPU_FAN, 28),
        "ring" | "touchpad" => (led_group::TOUCHPAD_RING, 30),
        "light1" | "logo" => (led_group::LIGHT_1, 40),
        "light" | "other" => (led_group::LIGHT_OTHER, 16),
        _ => return None,
    };
    Some(id | if on { 1u64 << bit } else { 0 })
}

pub fn led_color_payload(
    group: &str,
    rgb: Rgb,
    brightness: Option<u8>,
    enable: bool,
) -> Option<u64> {
    let index = match group {
        "cpufan" | "cpu-fan" | "cpu" => led_color_index::CPU_FAN,
        "gpufan" | "gpu-fan" | "gpu" | "heatpipe" | "heat-pipe" => led_color_index::GPU_FAN,
        "ring" | "touchpad" => led_color_index::TOUCHPAD_RING,
        "light1" | "logo" => led_color_index::LIGHT_1,
        "light" | "other" => led_color_index::LIGHT_OTHER,
        _ => return None,
    };
    let mut v = index | ((rgb.r as u64) << 8) | ((rgb.g as u64) << 16) | ((rgb.b as u64) << 24);
    if let Some(b) = brightness {
        v |= (b as u64) << 32;
    }
    if enable {
        v |= 1u64 << 40;
    }
    Some(v)
}

pub fn static_zone_payload(zone: u8, rgb: Rgb) -> [u8; 4] {
    [zone, rgb.r, rgb.g, rgb.b]
}

/// Keyboard effect ids from `LightingDynamicUI.GetHWEffect`.
///
/// The list order is the radio-button order in PredatorSense. Names are the
/// labels Acer ships in the UI; the last four are identified by their hardware
/// ids and left unnamed where no verified label was recovered.
pub const EFFECTS: &[(u8, &str)] = &[
    (2, "Static"),
    (5, "Breathing"),
    (11, "Neon"),
    (9, "Wave"),
    (4, "Ripple"),
    (12, "Rainbow"),
    (13, "Snake"),
    (3, "Zoom"),
    (8, "Spot"),
    (6, "Star"),
    (10, "Wave (cold)"),
    (17, "Effect 17"),
    (18, "Effect 18"),
    (19, "Effect 19"),
    (20, "Effect 20"),
    (21, "Effect 21"),
    (22, "Effect 22"),
];

pub fn effect_name(id: u8) -> &'static str {
    EFFECTS
        .iter()
        .find_map(|(i, n)| (*i == id).then_some(*n))
        .unwrap_or("Unknown effect")
}

pub fn effect_id(name: &str) -> Option<u8> {
    let needle = name.trim().to_ascii_lowercase();
    EFFECTS
        .iter()
        .find(|(_, n)| n.to_ascii_lowercase() == needle)
        .map(|(i, _)| *i)
}
