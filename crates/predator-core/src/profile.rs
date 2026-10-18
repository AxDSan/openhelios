use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::config;
use crate::error::{Error, Result};
use crate::lighting::{LightingMode, LightingState};
use crate::protocol::Rgb;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FanKind {
    Cpu,
    Cpu2,
    System,
    Gpu,
    Gpu2,
}

impl FanKind {
    pub const ALL: [FanKind; 5] = [
        FanKind::Cpu,
        FanKind::Cpu2,
        FanKind::System,
        FanKind::Gpu,
        FanKind::Gpu2,
    ];

    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "cpu" => Some(FanKind::Cpu),
            "cpu2" | "system" => Some(FanKind::Cpu2),
            "gpu" => Some(FanKind::Gpu),
            "gpu2" => Some(FanKind::Gpu2),
            "sys" | "sys1" => Some(FanKind::System),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            FanKind::Cpu => "CPU",
            FanKind::Cpu2 => "CPU 2",
            FanKind::System => "System",
            FanKind::Gpu => "GPU",
            FanKind::Gpu2 => "GPU 2",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FanSetting {
    pub auto: bool,
    pub percent: u8,
}

impl Default for FanSetting {
    fn default() -> Self {
        Self {
            auto: true,
            percent: 60,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct FanProfile {
    #[serde(default)]
    pub cpu: FanSetting,
    #[serde(default)]
    pub cpu2: FanSetting,
    #[serde(default)]
    pub gpu: FanSetting,
    #[serde(default)]
    pub gpu2: FanSetting,
    #[serde(default)]
    pub system: FanSetting,
}

impl FanProfile {
    pub fn get(&self, kind: FanKind) -> &FanSetting {
        match kind {
            FanKind::Cpu => &self.cpu,
            FanKind::Cpu2 => &self.cpu2,
            FanKind::System => &self.system,
            FanKind::Gpu => &self.gpu,
            FanKind::Gpu2 => &self.gpu2,
        }
    }

    pub fn get_mut(&mut self, kind: FanKind) -> &mut FanSetting {
        match kind {
            FanKind::Cpu => &mut self.cpu,
            FanKind::Cpu2 => &mut self.cpu2,
            FanKind::System => &mut self.system,
            FanKind::Gpu => &mut self.gpu,
            FanKind::Gpu2 => &mut self.gpu2,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LightingProfile {
    #[serde(default)]
    pub mode: LightingMode,
    #[serde(default = "default_brightness")]
    pub brightness: u8,
    #[serde(default = "default_color")]
    pub color: Rgb,
    #[serde(default = "default_effect")]
    pub effect: u8,
    #[serde(default = "default_speed")]
    pub speed: u8,
    #[serde(default = "default_direction")]
    pub direction: u8,
    #[serde(default)]
    pub keys: BTreeMap<u8, Rgb>,
}

fn default_brightness() -> u8 {
    50
}
fn default_color() -> Rgb {
    Rgb::DEFAULT_CYAN
}
fn default_effect() -> u8 {
    2
}
fn default_speed() -> u8 {
    5
}
fn default_direction() -> u8 {
    1
}

impl Default for LightingProfile {
    fn default() -> Self {
        Self {
            mode: LightingMode::Static,
            brightness: 50,
            color: Rgb::DEFAULT_CYAN,
            effect: 2,
            speed: 5,
            direction: 1,
            keys: BTreeMap::new(),
        }
    }
}

impl LightingProfile {
    pub fn state(&self) -> LightingState {
        LightingState::from_parts(
            self.mode,
            self.brightness,
            self.color,
            self.effect,
            self.speed,
            self.direction,
            &self.keys,
        )
    }

    pub fn from_state(state: &LightingState) -> Self {
        Self {
            mode: state.mode,
            brightness: state.brightness,
            color: state.color,
            effect: state.effect,
            speed: state.speed,
            direction: state.direction,
            keys: state.overrides(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RingProfile {
    pub enabled: bool,
    #[serde(default = "default_ring_brightness")]
    pub brightness: u8,
    #[serde(default = "default_ring_color")]
    pub color: Rgb,
}

fn default_ring_brightness() -> u8 {
    100
}
fn default_ring_color() -> Rgb {
    Rgb::new(0, 0, 255)
}

impl Default for RingProfile {
    fn default() -> Self {
        Self {
            enabled: false,
            brightness: 100,
            color: Rgb::new(0, 0, 255),
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PowerProfile {
    #[serde(default)]
    pub pl1_w: Option<f32>,
    #[serde(default)]
    pub pl2_w: Option<f32>,
    #[serde(default)]
    pub gpu_power_w: Option<f32>,
    #[serde(default)]
    pub gpu_lock_min_mhz: Option<f32>,
    #[serde(default)]
    pub gpu_lock_max_mhz: Option<f32>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct OcProfile {
    #[serde(default)]
    pub cpu_level: u8,
    #[serde(default)]
    pub gpu_level: u8,
    #[serde(default)]
    pub maxq_mode: Option<u8>,
    #[serde(default)]
    pub maxq_level: Option<u8>,
    #[serde(default)]
    pub gpu_mode: Option<u8>,
    #[serde(default)]
    pub post_sound: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Profile {
    #[serde(default = "default_name")]
    pub name: String,
    #[serde(default)]
    pub lighting: LightingProfile,
    #[serde(default)]
    pub ring: RingProfile,
    #[serde(default)]
    pub heatpipe: bool,
    #[serde(default)]
    pub fan: FanProfile,
    #[serde(default)]
    pub oc: OcProfile,
    #[serde(default)]
    pub power: PowerProfile,
}

fn default_name() -> String {
    "default".to_string()
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            name: default_name(),
            lighting: LightingProfile::default(),
            ring: RingProfile::default(),
            heatpipe: false,
            fan: FanProfile::default(),
            oc: OcProfile::default(),
            power: PowerProfile::default(),
        }
    }
}

impl Profile {
    pub fn lighting_state(&self) -> LightingState {
        self.lighting.state()
    }

    pub fn validate(&mut self) {
        self.lighting.brightness = self.lighting.brightness.min(100);
        self.lighting.speed = self.lighting.speed.min(10);
        self.lighting.direction = self.lighting.direction.min(1);
        self.ring.brightness = self.ring.brightness.min(100);
        for setting in [
            &mut self.fan.cpu,
            &mut self.fan.cpu2,
            &mut self.fan.gpu,
            &mut self.fan.gpu2,
            &mut self.fan.system,
        ] {
            setting.percent = setting.percent.min(100);
        }
        self.oc.cpu_level = self.oc.cpu_level.min(2);
        self.oc.gpu_level = self.oc.gpu_level.min(2);
        if let Some(v) = self.power.pl1_w {
            self.power.pl1_w = Some(v.clamp(5.0, 200.0));
        }
        if let Some(v) = self.power.pl2_w {
            self.power.pl2_w = Some(v.clamp(5.0, 200.0));
        }
        if let Some(v) = self.power.gpu_power_w {
            self.power.gpu_power_w = Some(v.clamp(5.0, 500.0));
        }
        if let Some(v) = self.power.gpu_lock_max_mhz {
            self.power.gpu_lock_max_mhz = Some(v.clamp(0.0, 4000.0));
        }
        if let Some(v) = self.power.gpu_lock_min_mhz {
            self.power.gpu_lock_min_mhz = Some(v.clamp(0.0, 4000.0));
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProfileStore {
    root: PathBuf,
}

impl Default for ProfileStore {
    fn default() -> Self {
        Self {
            root: config::profiles_dir(),
        }
    }
}

impl ProfileStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn path(&self, name: &str) -> PathBuf {
        self.root.join(format!("{name}.toml"))
    }

    pub fn list(&self) -> Result<Vec<String>> {
        let mut out = Vec::new();
        let entries = match fs::read_dir(&self.root) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
            Err(e) => return Err(Error::Io(e)),
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("toml") {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    out.push(stem.to_string());
                }
            }
        }
        out.sort();
        Ok(out)
    }

    pub fn load(&self, name: &str) -> Result<Profile> {
        let path = self.path(name);
        let text = fs::read_to_string(&path)
            .map_err(|e| Error::Config(format!("{}: {e}", path.display())))?;
        let mut p: Profile = toml::from_str(&text)?;
        if p.name.is_empty() {
            p.name = name.to_string();
        }
        p.validate();
        Ok(p)
    }

    pub fn load_or_default(&self, name: &str) -> Profile {
        self.load(name).unwrap_or_else(|_| Profile {
            name: name.to_string(),
            ..Default::default()
        })
    }

    pub fn save(&self, profile: &Profile) -> Result<PathBuf> {
        config::ensure_dirs()?;
        fs::create_dir_all(&self.root)?;
        let name = if profile.name.is_empty() {
            "default"
        } else {
            profile.name.as_str()
        };
        let mut p = profile.clone();
        p.name = name.to_string();
        p.validate();
        let path = self.path(name);
        fs::write(&path, toml::to_string_pretty(&p)?)?;
        Ok(path)
    }

    pub fn delete(&self, name: &str) -> Result<()> {
        fs::remove_file(self.path(name))?;
        Ok(())
    }

    pub fn exists(&self, name: &str) -> bool {
        self.path(name).exists()
    }
}

/// One JSON document containing every profile; used by `profile export`.
#[derive(Debug, Serialize, Deserialize)]
pub struct ProfileBundle {
    pub version: u32,
    pub profiles: Vec<Profile>,
}

impl ProfileStore {
    pub fn export(&self, path: &Path) -> Result<usize> {
        let mut profiles = Vec::new();
        for name in self.list()? {
            if let Ok(p) = self.load(&name) {
                profiles.push(p);
            }
        }
        let bundle = ProfileBundle {
            version: 1,
            profiles,
        };
        fs::write(path, serde_json::to_vec_pretty(&bundle)?)?;
        Ok(bundle.profiles.len())
    }

    pub fn import(&self, path: &Path) -> Result<usize> {
        let data = fs::read(path)?;
        let bundle: ProfileBundle = serde_json::from_slice(&data)?;
        let n = bundle.profiles.len();
        for p in bundle.profiles {
            self.save(&p)?;
        }
        Ok(n)
    }
}
