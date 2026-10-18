//! Linux sensor collection for the dashboard.
//!
//! Everything here is read-only. Values are gathered from hwmon, cpufreq,
//! RAPL, /proc and nvidia-smi; nothing is written.

use std::collections::VecDeque;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::wmi::AcerGaming;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct FanReading {
    pub name: String,
    pub rpm: u32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Sample {
    pub timestamp: String,
    pub cpu_temp: Option<f32>,
    pub cpu_cores: Vec<f32>,
    pub cpu_freq_mhz: Option<f32>,
    pub cpu_load: Option<f32>,
    pub cpu_power_w: Option<f32>,
    pub gpu_name: Option<String>,
    pub gpu_temp: Option<f32>,
    pub gpu_load: Option<f32>,
    pub gpu_clock_mhz: Option<f32>,
    pub gpu_power_w: Option<f32>,
    pub gpu_mem_used_mb: Option<f32>,
    pub gpu_mem_total_mb: Option<f32>,
    pub ec_cpu_temp: Option<f32>,
    pub ec_system_temp: Option<f32>,
    pub ec_gpu_temp: Option<f32>,
    pub cpu_fan_rpm: Option<u32>,
    pub gpu_fan_rpm: Option<u32>,
    pub cpu_fan_rate: Option<u32>,
    pub gpu_fan_rate: Option<u32>,
    pub slide_keyboard: Option<bool>,
    pub fans: Vec<FanReading>,
    pub battery_percent: Option<f32>,
    pub ac_online: Option<bool>,
    pub battery_status: Option<String>,
    pub mem_used_gb: Option<f32>,
    pub mem_total_gb: Option<f32>,
    pub uptime_s: Option<u64>,
}

#[derive(Clone, Debug, Default)]
pub struct HistoryPoint {
    pub cpu_temp: f32,
    pub gpu_temp: f32,
    pub cpu_load: f32,
    pub gpu_load: f32,
}

pub struct Monitor {
    last_cpu: Option<(u64, u64)>,
    last_energy: Option<(u64, Instant)>,
    pub history: VecDeque<HistoryPoint>,
    wmi: Option<AcerGaming>,
}

impl Default for Monitor {
    fn default() -> Self {
        Self {
            last_cpu: None,
            last_energy: None,
            history: VecDeque::with_capacity(180),
            wmi: AcerGaming::open().ok(),
        }
    }
}

impl Monitor {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn sample(&mut self) -> Sample {
        let mut s = Sample {
            timestamp: chrono::Local::now().to_rfc3339(),
            ..Default::default()
        };

        let hwmon = scan_hwmon();

        if let Some((package, cores)) = cpu_temps(&hwmon) {
            s.cpu_temp = Some(package);
            s.cpu_cores = cores;
        }

        s.cpu_freq_mhz = cpu_freq_mhz();
        s.cpu_load = self.cpu_load();

        if let Some(w) = self.rapl_power_w() {
            s.cpu_power_w = Some(w);
        }

        if let Some(g) = nvidia_smi() {
            s.gpu_name = Some(g.name);
            s.gpu_temp = g.temp;
            s.gpu_load = g.load;
            s.gpu_clock_mhz = g.clock;
            s.gpu_power_w = g.power;
            s.gpu_mem_used_mb = g.mem_used;
            s.gpu_mem_total_mb = g.mem_total;
        } else if let Some(t) = gpu_hwmon_temp(&hwmon) {
            s.gpu_temp = Some(t);
        }

        if let Some(w) = &self.wmi {
            let health = |idx: u8| w.get_health(idx).ok().flatten();
            s.ec_cpu_temp = health(1).map(|v| v as f32);
            s.cpu_fan_rpm = health(2);
            s.ec_system_temp = health(3).map(|v| v as f32);
            s.gpu_fan_rpm = health(6);
            s.ec_gpu_temp = health(10).map(|v| v as f32);
            s.cpu_fan_rate = health(12);
            s.gpu_fan_rate = health(16);
            s.slide_keyboard = w.get_slide_keyboard().ok();
            if let Some(t) = s.ec_cpu_temp {
                s.cpu_temp = Some(t);
            }
            if let Some(t) = s.ec_gpu_temp {
                s.gpu_temp = Some(t);
            }
        }

        s.fans = fan_readings(&hwmon);
        if let Some(rpm) = s.gpu_fan_rpm {
            s.fans.insert(
                0,
                FanReading {
                    name: "EC GPU".into(),
                    rpm,
                },
            );
        }
        if let Some(rpm) = s.cpu_fan_rpm {
            s.fans.insert(
                0,
                FanReading {
                    name: "EC CPU".into(),
                    rpm,
                },
            );
        }
        let (ac, batt, status) = power_supply();
        s.ac_online = ac;
        s.battery_percent = batt;
        s.battery_status = status;
        let (used, total) = memory_gb();
        s.mem_used_gb = used;
        s.mem_total_gb = total;
        s.uptime_s = uptime_s();

        self.history.push_back(HistoryPoint {
            cpu_temp: s.cpu_temp.unwrap_or(0.0),
            gpu_temp: s.gpu_temp.unwrap_or(0.0),
            cpu_load: s.cpu_load.unwrap_or(0.0),
            gpu_load: s.gpu_load.unwrap_or(0.0),
        });
        while self.history.len() > 180 {
            self.history.pop_front();
        }

        s
    }

    fn cpu_load(&mut self) -> Option<f32> {
        let text = fs::read_to_string("/proc/stat").ok()?;
        let line = text.lines().next()?;
        let mut it = line.split_whitespace();
        if it.next()? != "cpu" {
            return None;
        }
        let vals: Vec<u64> = it.filter_map(|v| v.parse().ok()).collect();
        if vals.len() < 4 {
            return None;
        }
        let total: u64 = vals.iter().sum();
        let idle = vals[3] + vals.get(4).copied().unwrap_or(0);
        let delta = match self.last_cpu {
            Some((lt, li)) if total > lt && idle >= li => {
                let dt = total - lt;
                let di = idle - li;
                if dt == 0 {
                    None
                } else {
                    Some(((dt - di) as f32 / dt as f32) * 100.0)
                }
            }
            _ => None,
        };
        self.last_cpu = Some((total, idle));
        delta
    }

    fn rapl_power_w(&mut self) -> Option<f32> {
        let energy = read_u64(Path::new("/sys/class/powercap/intel-rapl:0/energy_uj"))?;
        let max = read_u64(Path::new(
            "/sys/class/powercap/intel-rapl:0/max_energy_range_uj",
        ))
        .unwrap_or(u64::MAX);
        let now = Instant::now();
        let out = self.last_energy.and_then(|(last, then)| {
            let dt = now.duration_since(then).as_secs_f32();
            if dt <= 0.05 {
                return None;
            }
            let delta = if energy >= last {
                energy - last
            } else {
                max.saturating_sub(last).saturating_add(energy)
            };
            Some(delta as f32 / 1_000_000.0 / dt)
        });
        self.last_energy = Some((energy, now));
        out
    }
}

#[derive(Debug)]
struct GpuSample {
    name: String,
    temp: Option<f32>,
    load: Option<f32>,
    clock: Option<f32>,
    power: Option<f32>,
    mem_used: Option<f32>,
    mem_total: Option<f32>,
}

fn nvidia_smi() -> Option<GpuSample> {
    let out = Command::new("nvidia-smi")
        .args([
            "--query-gpu=name,temperature.gpu,utilization.gpu,clocks.current.graphics,power.draw,memory.used,memory.total",
            "--format=csv,noheader,nounits",
        ])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let line = text.lines().next()?;
    let fields: Vec<&str> = line.split(',').map(str::trim).collect();
    let num = |i: usize| fields.get(i).and_then(|v| v.parse::<f32>().ok());
    Some(GpuSample {
        name: fields.first().unwrap_or(&"NVIDIA GPU").to_string(),
        temp: num(1),
        load: num(2),
        clock: num(3),
        power: num(4),
        mem_used: num(5),
        mem_total: num(6),
    })
}

#[derive(Debug)]
struct Hwmon {
    name: String,
    path: PathBuf,
}

fn scan_hwmon() -> Vec<Hwmon> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir("/sys/class/hwmon") else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = fs::read_to_string(path.join("name"))
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|_| "hwmon".to_string());
        out.push(Hwmon { name, path });
    }
    out
}

fn read_millideg(path: &Path) -> Option<f32> {
    read_u64(path).map(|v| v as f32 / 1000.0)
}

fn cpu_temps(hwmon: &[Hwmon]) -> Option<(f32, Vec<f32>)> {
    let cpu = hwmon.iter().find(|h| {
        h.name.contains("coretemp") || h.name.contains("k10temp") || h.name.contains("zenpower")
    })?;
    let mut cores = Vec::new();
    for i in 1..=32 {
        let p = cpu.path.join(format!("temp{i}_input"));
        if !p.exists() {
            continue;
        }
        if let Some(t) = read_millideg(&p) {
            if t > 1.0 {
                cores.push(t);
            }
        }
    }
    if cores.is_empty() {
        return None;
    }
    // coretemp labels temp1 as Package id 0 on this CPU family; keep it as the
    // primary value and expose all cores separately.
    let package = cores[0];
    Some((package, cores))
}

fn gpu_hwmon_temp(hwmon: &[Hwmon]) -> Option<f32> {
    let gpu = hwmon.iter().find(|h| {
        let n = h.name.to_ascii_lowercase();
        n.contains("amdgpu") || n.contains("nouveau") || n.contains("nvidia")
    })?;
    read_millideg(&gpu.path.join("temp1_input"))
}

fn fan_readings(hwmon: &[Hwmon]) -> Vec<FanReading> {
    let mut out = Vec::new();
    for h in hwmon {
        for i in 1..=8 {
            let p = h.path.join(format!("fan{i}_input"));
            if let Some(rpm) = read_u64(&p) {
                out.push(FanReading {
                    name: format!("{} fan{}", h.name, i),
                    rpm: rpm.min(u32::MAX as u64) as u32,
                });
            }
        }
    }
    out
}

fn cpu_freq_mhz() -> Option<f32> {
    let mut sum = 0u64;
    let mut n = 0u64;
    let base = Path::new("/sys/devices/system/cpu");
    if let Ok(entries) = fs::read_dir(base) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if !name.starts_with("cpu") || !name[3..].chars().all(|c| c.is_ascii_digit()) {
                continue;
            }
            let p = entry.path().join("cpufreq/scaling_cur_freq");
            if let Some(khz) = read_u64(&p) {
                sum += khz;
                n += 1;
            }
        }
    }
    if n > 0 {
        return Some(sum as f32 / n as f32 / 1000.0);
    }
    let text = fs::read_to_string("/proc/cpuinfo").ok()?;
    let mhz: Vec<f32> = text
        .lines()
        .filter_map(|l| l.strip_prefix("cpu MHz"))
        .filter_map(|l| l.split(':').nth(1))
        .filter_map(|v| v.trim().parse().ok())
        .collect();
    if mhz.is_empty() {
        None
    } else {
        Some(mhz.iter().sum::<f32>() / mhz.len() as f32)
    }
}

fn power_supply() -> (Option<bool>, Option<f32>, Option<String>) {
    let mut ac = None;
    let mut battery = None;
    let mut status = None;
    let Ok(entries) = fs::read_dir("/sys/class/power_supply") else {
        return (ac, battery, status);
    };
    for entry in entries.flatten() {
        let p = entry.path();
        let kind = read_string(&p.join("type")).unwrap_or_default();
        if kind == "Mains" || kind == "USB" {
            let online = read_u64(&p.join("online")).map(|v| v != 0);
            ac = Some(ac.unwrap_or(false) || online.unwrap_or(false));
        } else if kind == "Battery" && battery.is_none() {
            battery = read_u64(&p.join("capacity")).map(|v| v as f32);
            status = read_string(&p.join("status"));
        }
    }
    (ac, battery, status)
}

fn memory_gb() -> (Option<f32>, Option<f32>) {
    let Ok(text) = fs::read_to_string("/proc/meminfo") else {
        return (None, None);
    };
    let mut total = None;
    let mut avail = None;
    for line in text.lines() {
        if let Some(v) = line.strip_prefix("MemTotal:") {
            total = v
                .split_whitespace()
                .next()
                .and_then(|v| v.parse::<f32>().ok());
        } else if let Some(v) = line.strip_prefix("MemAvailable:") {
            avail = v
                .split_whitespace()
                .next()
                .and_then(|v| v.parse::<f32>().ok());
        }
    }
    match (total, avail) {
        (Some(t), Some(a)) => (Some((t - a) / 1024.0 / 1024.0), Some(t / 1024.0 / 1024.0)),
        (Some(t), None) => (None, Some(t / 1024.0 / 1024.0)),
        _ => (None, None),
    }
}

fn uptime_s() -> Option<u64> {
    let text = fs::read_to_string("/proc/uptime").ok()?;
    text.split_whitespace()
        .next()?
        .parse::<f64>()
        .ok()
        .map(|v| v as u64)
}

fn read_string(path: &Path) -> Option<String> {
    fs::read_to_string(path).ok().map(|s| s.trim().to_string())
}

fn read_u64(path: &Path) -> Option<u64> {
    fs::read_to_string(path).ok()?.trim().parse().ok()
}
