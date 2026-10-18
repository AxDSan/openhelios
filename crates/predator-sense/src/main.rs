use std::thread::sleep;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};
use predator_core::keymap::{self, KeyDef};
use predator_core::lighting::{LightingMode, LightingState};
use predator_core::profile::{FanKind, OcProfile, Profile, ProfileStore};
use predator_core::protocol::{Rgb, EFFECTS};
use predator_core::safety::{self, SafetyPolicy};
use predator_core::wmi::hexs;
use predator_core::{oc, Controller, Monitor};

#[derive(Parser, Debug)]
#[command(
    name = "predator-sense",
    version,
    about = "PredatorSense Linux (unofficial) — Acer Predator PH717 lighting, fan and OC control",
    long_about = None
)]
struct Cli {
    /// Global write switch: inspect only.
    #[arg(long, global = true)]
    read_only: bool,
    /// Print what would be written without touching hardware.
    #[arg(long, global = true)]
    dry_run: bool,
    /// Permit EC overclock writes.
    #[arg(long, global = true)]
    allow_oc: bool,
    /// Permit raw/unknown WMI method ids.
    #[arg(long, global = true)]
    force: bool,
    /// Do not snapshot before applying a profile.
    #[arg(long, global = true)]
    no_snapshot: bool,
    /// Machine-readable output where supported.
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Show device and permission status.
    Doctor,
    /// Print one or more sensor samples.
    Monitor {
        #[arg(long, default_value_t = 1)]
        count: u32,
        #[arg(long, default_value_t = 1)]
        interval: u64,
    },
    /// Keyboard lighting (per-key HID).
    Lighting(LightingCmd),
    /// Touchpad ring (WMI method 10/12).
    Ring {
        state: OnOff,
        #[arg(long, default_value_t = 100)]
        brightness: u8,
        #[arg(long, default_value = "#0000FF")]
        color: String,
    },
    /// Heat-pipe light (WMI method 10).
    Heatpipe { state: OnOff },
    /// Fan control (WMI methods 14/16).
    Fan(FanCmd),
    /// Firmware overclock levels and Linux CPU controls.
    Oc(OcCmd),
    /// Send a raw WMI method id and payload.
    Raw {
        method: u8,
        #[arg(trailing_var_arg = true)]
        bytes: Vec<String>,
    },
    /// Raw HID feature commands.
    Hid(HidCmd),
    /// MagForce key actuation, switch mode, analog curve and game keys.
    Magforce(MagforceCmd),
    /// Capture a recovery snapshot.
    Snapshot {
        #[arg(long, default_value = "default")]
        profile: String,
    },
    /// Restore the latest snapshot (safe state by default).
    Restore {
        #[arg(long)]
        apply_profile: bool,
    },
    /// Immediate safe state: lights off, fans auto, OC normal.
    Panic,
    /// Manage profiles.
    Profile(ProfileCmd),
    /// Print udev / DKMS installation hints.
    InstallHints,
}

#[derive(Args, Debug)]
struct LightingCmd {
    #[command(subcommand)]
    cmd: LightingSub,
}

#[derive(Subcommand, Debug)]
enum LightingSub {
    /// Static per-key colours.
    Static {
        #[arg(long, default_value = "#00AEC7")]
        color: String,
        #[arg(long, default_value_t = 50)]
        brightness: u8,
        /// Comma-separated hardware tags (see `lighting list`).
        #[arg(long)]
        keys: Option<String>,
        /// all, wasd, arrows, fkeys, numbers, numpad, modifiers.
        #[arg(long)]
        preset: Option<String>,
    },
    /// Animated effect.
    Effect {
        #[arg(long, conflicts_with = "name")]
        id: Option<u8>,
        #[arg(long, conflicts_with = "id")]
        name: Option<String>,
        #[arg(long, default_value_t = 5)]
        speed: u8,
        #[arg(long, default_value_t = 1)]
        direction: u8,
        #[arg(long, default_value = "#00AEC7")]
        color: String,
        #[arg(long, default_value_t = 50)]
        brightness: u8,
    },
    /// Brightness-only update (PredatorSense 51/5/1 path).
    Brightness { value: u8 },
    /// Turn the keyboard backlight off.
    Off,
    /// Change one key by hardware tag.
    Key { tag: u8, color: String },
    /// List key tags, names, coordinates and effects.
    List,
}

#[derive(Args, Debug)]
struct FanCmd {
    #[command(subcommand)]
    cmd: FanSub,
}

#[derive(Subcommand, Debug)]
enum FanSub {
    /// Return every fan to automatic control.
    Auto,
    /// Quiet preset: CPU/GPU manual 35%.
    Quiet,
    /// Performance preset: CPU/GPU manual 90%.
    Performance,
    /// Set one fan group manually.
    Set {
        #[arg(long)]
        group: String,
        #[arg(long)]
        percent: u8,
    },
    /// Read back the WMI fan behaviour/speed registers.
    Status,
}

#[derive(Args, Debug)]
struct OcCmd {
    #[command(subcommand)]
    cmd: OcSub,
}

#[derive(Subcommand, Debug)]
enum OcSub {
    /// CPU firmware OC level: 0 normal, 1 fast, 2 turbo.
    Cpu { level: u8 },
    /// GPU firmware OC level: 0 normal, 1 fast, 2 turbo.
    Gpu { level: u8 },
    /// Max-Q mode/level.
    Maxq {
        #[arg(long)]
        mode: u8,
        #[arg(long)]
        level: u8,
    },
    /// GPU mode selector (PredatorSense `Set_System_GPUMode`).
    GpuMode { mode: u8 },
    /// POST animation sound.
    Sound { state: OnOff },
    /// Linux cpufreq governor / EPP / turbo controls.
    Linux {
        #[command(subcommand)]
        cmd: LinuxSub,
    },
    /// Linux CPU package power limits (RAPL PL1/PL2).
    Power {
        #[command(subcommand)]
        cmd: PowerSub,
    },
    /// NVIDIA power limit and locked clocks.
    Nvidia {
        #[command(subcommand)]
        cmd: GpuSub,
    },
    /// Show firmware and Linux CPU state.
    Status,
}

#[derive(Subcommand, Debug)]
enum PowerSub {
    Status,
    Pl1 {
        watts: f32,
        #[arg(long)]
        window_ms: Option<u64>,
    },
    Pl2 {
        watts: f32,
        #[arg(long)]
        window_ms: Option<u64>,
    },
}

#[derive(Subcommand, Debug)]
enum GpuSub {
    Status,
    Power { watts: f32 },
    Lock { min: f32, max: f32 },
    Reset,
}

#[derive(Subcommand, Debug)]
enum LinuxSub {
    Status,
    Governor { value: String },
    Epp { value: String },
    NoTurbo { state: OnOff },
    MaxFreq { khz: u64 },
}

#[derive(Args, Debug)]
struct MagforceCmd {
    #[command(subcommand)]
    cmd: MagforceSub,
}

#[derive(Subcommand, Debug)]
enum MagforceSub {
    /// Actuation point (agent command 0x32, raw 0..255).
    Actuation { value: u8 },
    /// Switch/linear/joystick setting (agent command 0x31).
    Setting {
        #[arg(long, default_value_t = 0)]
        switch_mode: u8,
        #[arg(long, default_value_t = 1)]
        linear_mode: u8,
        #[arg(long, default_value_t = 0)]
        joystick: u8,
    },
    /// Analog curve mode (agent command 0x35).
    Analog { mode: u8 },
    /// MagForce replace mode (agent command 0x30).
    Replace {
        state: OnOff,
        #[arg(long)]
        backlight_30s: bool,
    },
    /// Assign four hardware tags to W/A/S/D game keys.
    Gamekeys {
        #[arg(long, num_args = 4, value_delimiter = ',')]
        keys: Vec<u8>,
    },
    /// Read MagForce getters.
    Status,
}

#[derive(Args, Debug)]
struct HidCmd {
    #[command(subcommand)]
    cmd: HidSub,
}

#[derive(Subcommand, Debug)]
enum HidSub {
    /// Send an 8-byte command whose first byte is `opcode`; print the response.
    Get { opcode: u8 },
    /// Send 8 raw command bytes.
    Raw {
        #[arg(trailing_var_arg = true)]
        bytes: Vec<String>,
    },
    /// Print the 0x88 and 0xB0 getter responses.
    Status,
}

#[derive(Args, Debug)]
struct ProfileCmd {
    #[command(subcommand)]
    cmd: ProfileSub,
}

#[derive(Subcommand, Debug)]
enum ProfileSub {
    List,
    Show {
        name: String,
    },
    /// Save a profile from the built-in safe defaults (or with --from-file).
    Save {
        name: String,
        #[arg(long)]
        from_file: Option<String>,
    },
    Apply {
        name: String,
    },
    Delete {
        name: String,
    },
    Export {
        path: String,
    },
    Import {
        path: String,
    },
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum OnOff {
    On,
    Off,
}

impl OnOff {
    fn is_on(self) -> bool {
        matches!(self, OnOff::On)
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let policy = SafetyPolicy {
        allow_writes: !cli.read_only,
        allow_oc: cli.allow_oc,
        dry_run: cli.dry_run,
        force: cli.force,
        snapshot_before_write: !cli.no_snapshot,
    };
    let mut c = Controller::detect(policy);

    let hardware_write = matches!(
        &cli.cmd,
        Cmd::Lighting(_)
            | Cmd::Ring { .. }
            | Cmd::Heatpipe { .. }
            | Cmd::Fan(_)
            | Cmd::Oc(_)
            | Cmd::Raw { .. }
            | Cmd::Hid(_)
            | Cmd::Magforce(_)
    );
    if hardware_write && c.policy.snapshot_before_write && !c.policy.dry_run {
        let p = Profile::default();
        if let Err(e) = c.save_state(&p) {
            eprintln!("warning: pre-write snapshot failed: {e}");
        }
    }

    let result = dispatch(&cli, &mut c);
    for line in &c.log {
        eprintln!("{line}");
    }
    result
}

fn dispatch(cli: &Cli, c: &mut Controller) -> Result<()> {
    match &cli.cmd {
        Cmd::Doctor => doctor(c, cli.json),
        Cmd::Monitor { count, interval } => {
            let mut monitor = Monitor::new();
            for i in 0..*count {
                let s = monitor.sample();
                if cli.json {
                    println!("{}", serde_json::to_string_pretty(&s)?);
                } else {
                    print_sample(&s);
                }
                if i + 1 < *count {
                    sleep(Duration::from_secs(*interval));
                }
            }
            Ok(())
        }
        Cmd::Lighting(l) => lighting(c, &l.cmd),
        Cmd::Ring {
            state,
            brightness,
            color,
        } => {
            let rgb = parse_rgb(color)?;
            c.apply_ring(state.is_on(), *brightness, rgb)
                .map_err(Into::into)
        }
        Cmd::Heatpipe { state } => c.apply_heatpipe(state.is_on()).map_err(Into::into),
        Cmd::Fan(f) => fan(c, &f.cmd),
        Cmd::Oc(o) => oc_cmd(c, &o.cmd),
        Cmd::Raw { method, bytes } => {
            if *method > 25 && !c.policy.force {
                bail!("method {method} is outside the known 0..=25 range; pass --force");
            }
            let payload = parse_bytes(bytes)?;
            let wmi = c
                .wmi
                .as_ref()
                .context("WMI device unavailable; load acer_gaming_probe")?;
            let resp = wmi.raw(*method, &payload)?;
            println!("method {method} payload {}", hexs(&payload));
            println!("response {}", hexs(&resp));
            Ok(())
        }
        Cmd::Hid(h) => hid(c, &h.cmd),
        Cmd::Magforce(m) => magforce(c, &m.cmd),
        Cmd::Snapshot { profile } => {
            let store = ProfileStore::default();
            let p = store.load_or_default(profile);
            let path = c.save_state(&p)?;
            println!("{}", path.display());
            Ok(())
        }
        Cmd::Restore { apply_profile } => c.restore_latest(*apply_profile).map_err(Into::into),
        Cmd::Panic => c.panic().map_err(Into::into),
        Cmd::Profile(p) => profile_cmd(c, &p.cmd),
        Cmd::InstallHints => {
            install_hints();
            Ok(())
        }
    }
}

fn doctor(c: &Controller, json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(&c.status)?);
        return Ok(());
    }
    println!("model         : {}", c.status.model);
    println!("board         : {}", c.status.board);
    println!("bios          : {}", c.status.bios);
    println!("kernel        : {}", c.status.kernel);
    println!(
        "WMI           : {} ({})",
        if c.status.wmi { "ok" } else { "unavailable" },
        c.status.wmi_path
    );
    if let Some(e) = &c.status.wmi_error {
        println!("  error       : {e}");
    }
    println!(
        "HID keyboard  : {}",
        if c.status.hid { "ok" } else { "unavailable" }
    );
    if let Some(p) = &c.status.hid_path {
        println!("  path        : {p}");
    }
    if let Some(p) = &c.status.hid_phys {
        println!("  phys        : {p}");
    }
    if let Some(h) = &c.status.hid_health {
        println!("  health 0x88 : {h}");
    }
    if let Some(h) = &c.status.hid_b0 {
        println!("  status 0xB0 : {h}");
    }
    if let Some(e) = &c.status.hid_error {
        println!("  error       : {e}");
    }
    println!(
        "nvidia-smi    : {}",
        if c.status.nvidia_smi { "ok" } else { "missing" }
    );
    let cpu = oc::cpu_status();
    println!(
        "cpufreq       : {:?} governor={:?} epp={:?}",
        cpu.driver, cpu.governor, cpu.epp
    );
    if c.status.hid_error.is_some() || c.status.wmi_error.is_some() {
        println!();
        install_hints();
    }
    Ok(())
}

fn print_sample(s: &predator_core::Sample) {
    let opt = |v: Option<f32>| v.map(|x| format!("{x:.1}")).unwrap_or_else(|| "—".into());
    println!(
        "{}  CPU {}°C {}MHz load {}% {}W | GPU {} {}°C {}% {}MHz {}W | fans {} | RAM {}/{}GB | AC {} batt {}% {}",
        s.timestamp,
        opt(s.cpu_temp),
        opt(s.cpu_freq_mhz),
        opt(s.cpu_load),
        opt(s.cpu_power_w),
        s.gpu_name.as_deref().unwrap_or("—"),
        opt(s.gpu_temp),
        opt(s.gpu_load),
        opt(s.gpu_clock_mhz),
        opt(s.gpu_power_w),
        if s.fans.is_empty() {
            "—".to_string()
        } else {
            s.fans
                .iter()
                .map(|f| format!("{}={}rpm", f.name, f.rpm))
                .collect::<Vec<_>>()
                .join(",")
        },
        opt(s.mem_used_gb),
        opt(s.mem_total_gb),
        match s.ac_online {
            Some(true) => "on",
            Some(false) => "off",
            None => "—",
        },
        opt(s.battery_percent),
        s.battery_status.as_deref().unwrap_or(""),
    );
}

fn lighting(c: &mut Controller, cmd: &LightingSub) -> Result<()> {
    match cmd {
        LightingSub::Static {
            color,
            brightness,
            keys,
            preset,
        } => {
            let rgb = parse_rgb(color)?;
            let mut state = LightingState {
                mode: LightingMode::Static,
                brightness: *brightness,
                ..Default::default()
            };
            state.fill(rgb);
            if let Some(p) = preset {
                let tags = keymap::preset(p);
                if tags.is_empty() {
                    bail!("unknown preset {p:?}; use one of {:?}", keymap::PRESETS);
                }
                // Presets paint the selection over a dark base, like the Windows UI.
                state.fill(Rgb::BLACK);
                state.set_keys(&tags, rgb);
            }
            if let Some(list) = keys {
                let tags = parse_tags(list)?;
                state.set_keys(&tags, rgb);
            }
            c.apply_lighting(state).map_err(Into::into)
        }
        LightingSub::Effect {
            id,
            name,
            speed,
            direction,
            color,
            brightness,
        } => {
            let effect = match (id, name) {
                (Some(v), _) => *v,
                (None, Some(n)) => predator_core::protocol::effect_id(n)
                    .with_context(|| format!("unknown effect {n:?}; see `lighting list`"))?,
                (None, None) => 2,
            };
            let state = LightingState {
                mode: LightingMode::Effect,
                effect,
                speed: *speed,
                direction: *direction,
                color: parse_rgb(color)?,
                brightness: *brightness,
                ..Default::default()
            };
            c.apply_lighting(state).map_err(Into::into)
        }
        LightingSub::Brightness { value } => {
            c.policy.require_writes()?;
            if c.policy.dry_run {
                c.note(format!("dry-run: keyboard brightness {value}"));
                return Ok(());
            }
            let hid = c.hid.as_ref().context("keyboard HID device unavailable")?;
            hid.brightness(*value)?;
            c.note(format!("keyboard brightness {value}"));
            Ok(())
        }
        LightingSub::Off => {
            let state = LightingState {
                mode: LightingMode::Off,
                ..Default::default()
            };
            c.apply_lighting(state).map_err(Into::into)
        }
        LightingSub::Key { tag, color } => {
            let rgb = parse_rgb(color)?;
            let mut state = LightingState::default();
            state.fill(Rgb::BLACK);
            if *tag as usize >= keymap::SLOT_COUNT {
                bail!("tag must be 0..={}", keymap::SLOT_COUNT - 1);
            }
            state.set_keys(&[*tag], rgb);
            c.apply_lighting(state).map_err(Into::into)
        }
        LightingSub::List => {
            println!("tag  name                       x    y    w   h   label");
            for k in keymap::keys() {
                print_key(k);
            }
            println!("\neffects:");
            for (id, name) in EFFECTS {
                println!("  {id:3}  {name}");
            }
            Ok(())
        }
    }
}

fn print_key(k: &KeyDef) {
    println!(
        "{:3}  {:26} {:4} {:4} {:3} {:3}  {}",
        k.tag,
        k.name,
        k.x as i32,
        k.y as i32,
        k.w as i32,
        k.h as i32,
        keymap::label(k)
    );
}

fn fan(c: &mut Controller, cmd: &FanSub) -> Result<()> {
    match cmd {
        FanSub::Auto => {
            for kind in [FanKind::Cpu, FanKind::Gpu] {
                c.apply_fan(kind, true, 0)?;
            }
            Ok(())
        }
        FanSub::Quiet => {
            for kind in [FanKind::Cpu, FanKind::Gpu] {
                c.apply_fan(kind, false, 35)?;
            }
            Ok(())
        }
        FanSub::Performance => {
            for kind in [FanKind::Cpu, FanKind::Gpu] {
                c.apply_fan(kind, false, 90)?;
            }
            Ok(())
        }
        FanSub::Set { group, percent } => {
            let kind =
                FanKind::parse(group).with_context(|| format!("unknown fan group {group:?}"))?;
            c.apply_fan(kind, false, *percent).map_err(Into::into)
        }
        FanSub::Status => {
            let wmi = c.wmi.as_ref().context("WMI device unavailable")?;
            for (label, sel) in [
                ("cpu", 1u64),
                ("sys1", 2),
                ("sys2", 3),
                ("gpu1", 4),
                ("gpu2", 5),
            ] {
                let beh = wmi.get_fan_behavior(sel).unwrap_or(0);
                let spd = wmi.get_fan_speed(sel).unwrap_or(0);
                println!(
                    "{label:5} behavior=0x{beh:016X} speed=0x{spd:016X} percent={}",
                    (spd >> 8) & 0xFF
                );
            }
            Ok(())
        }
    }
}

fn oc_cmd(c: &mut Controller, cmd: &OcSub) -> Result<()> {
    match cmd {
        OcSub::Cpu { level } => oc_one(
            c,
            OcProfile {
                cpu_level: *level,
                ..Default::default()
            },
        ),
        OcSub::Gpu { level } => oc_one(
            c,
            OcProfile {
                gpu_level: *level,
                ..Default::default()
            },
        ),
        OcSub::Maxq { mode, level } => oc_one(
            c,
            OcProfile {
                maxq_mode: Some(*mode),
                maxq_level: Some(*level),
                ..Default::default()
            },
        ),
        OcSub::GpuMode { mode } => oc_one(
            c,
            OcProfile {
                gpu_mode: Some(*mode),
                ..Default::default()
            },
        ),
        OcSub::Sound { state } => oc_one(
            c,
            OcProfile {
                post_sound: Some(state.is_on()),
                ..Default::default()
            },
        ),
        OcSub::Linux { cmd } => {
            c.policy.require_writes()?;
            if c.policy.dry_run {
                c.note(format!("dry-run: linux oc {cmd:?}"));
                return Ok(());
            }
            match cmd {
                LinuxSub::Status => {
                    print_cpu_status();
                    Ok(())
                }
                LinuxSub::Governor { value } => {
                    let n = oc::set_governor(value)?;
                    println!("set governor {value} on {n} policies");
                    Ok(())
                }
                LinuxSub::Epp { value } => {
                    let n = oc::set_epp(value)?;
                    println!("set EPP {value} on {n} policies");
                    Ok(())
                }
                LinuxSub::NoTurbo { state } => {
                    oc::set_no_turbo(!state.is_on())?;
                    println!("no_turbo={}", if state.is_on() { 0 } else { 1 });
                    Ok(())
                }
                LinuxSub::MaxFreq { khz } => {
                    let n = oc::set_max_freq_khz(*khz)?;
                    println!("set max freq {khz} kHz on {n} policies");
                    Ok(())
                }
            }
        }
        OcSub::Power { cmd } => oc_power(c, cmd),
        OcSub::Nvidia { cmd } => oc_gpu(c, cmd),
        OcSub::Status => {
            print_cpu_status();
            print_power_status();
            if let Some(wmi) = &c.wmi {
                for sel in [2u64, 3, 6, 9, 270] {
                    match wmi.get_misc(sel) {
                        Ok(v) => println!("misc[{sel}] = 0x{v:016X}"),
                        Err(e) => println!("misc[{sel}] error: {e}"),
                    }
                }
            }
            Ok(())
        }
    }
}

fn print_power_status() {
    let p = oc::PowerLimits::read();
    println!(
        "RAPL PL1={:?} W window={:?} us  PL2={:?} W window={:?} us",
        p.pl1_uw.map(|v| v as f64 / 1e6),
        p.pl1_window_us,
        p.pl2_uw.map(|v| v as f64 / 1e6),
        p.pl2_window_us
    );
    if let Some(n) = oc::nvidia_status() {
        println!(
            "GPU power limit={:?} W (min {:?}, max {:?}) clock max={:?} MHz current={:?} MHz",
            n.power_limit_w, n.min_power_w, n.max_power_w, n.max_clock_mhz, n.current_clock_mhz
        );
    } else {
        println!("GPU: nvidia-smi unavailable");
    }
}

fn oc_power(c: &mut Controller, cmd: &PowerSub) -> Result<()> {
    match cmd {
        PowerSub::Status => {
            print_power_status();
            Ok(())
        }
        PowerSub::Pl1 { watts, window_ms } => {
            c.policy.require_oc()?;
            if c.policy.dry_run {
                c.note(format!("dry-run: PL1 {watts} W"));
                return Ok(());
            }
            let p = oc::PowerLimits::read();
            p.set_pl1(*watts, *window_ms)?;
            c.note(format!("PL1 set to {watts} W"));
            Ok(())
        }
        PowerSub::Pl2 { watts, window_ms } => {
            c.policy.require_oc()?;
            if c.policy.dry_run {
                c.note(format!("dry-run: PL2 {watts} W"));
                return Ok(());
            }
            let p = oc::PowerLimits::read();
            p.set_pl2(*watts, *window_ms)?;
            c.note(format!("PL2 set to {watts} W"));
            Ok(())
        }
    }
}

fn oc_gpu(c: &mut Controller, cmd: &GpuSub) -> Result<()> {
    match cmd {
        GpuSub::Status => {
            if let Some(n) = oc::nvidia_status() {
                println!("{n:?}");
            } else {
                println!("nvidia-smi unavailable");
            }
            Ok(())
        }
        GpuSub::Power { watts } => {
            c.policy.require_oc()?;
            if c.policy.dry_run {
                c.note(format!("dry-run: GPU power limit {watts} W"));
                return Ok(());
            }
            oc::set_nvidia_power_limit(*watts)?;
            c.note(format!("GPU power limit set to {watts} W"));
            Ok(())
        }
        GpuSub::Lock { min, max } => {
            c.policy.require_oc()?;
            if c.policy.dry_run {
                c.note(format!("dry-run: GPU locked clocks {min}..{max} MHz"));
                return Ok(());
            }
            oc::set_gpu_locked_clocks(*min, *max)?;
            c.note(format!("GPU clocks locked to {min}..{max} MHz"));
            Ok(())
        }
        GpuSub::Reset => {
            c.policy.require_oc()?;
            if c.policy.dry_run {
                c.note("dry-run: reset GPU clocks");
                return Ok(());
            }
            oc::reset_gpu_clocks()?;
            c.note("GPU clocks reset");
            Ok(())
        }
    }
}

fn oc_one(c: &mut Controller, p: OcProfile) -> Result<()> {
    c.apply_oc(&p).map_err(anyhow::Error::from)
}

fn print_cpu_status() {
    let s = oc::cpu_status();
    println!(
        "governor={:?} available={:?} epp={:?} available_epp={:?}",
        s.governor, s.available_governors, s.epp, s.available_epp
    );
    println!(
        "driver={:?} no_turbo={:?} max_freq={:?} cur_freq={:?} msr={}",
        s.driver, s.no_turbo, s.max_freq_khz, s.cur_freq_khz, s.msr_available
    );
}

fn magforce(c: &mut Controller, cmd: &MagforceSub) -> Result<()> {
    let hid = c.hid.as_ref().context("keyboard HID device unavailable")?;
    match cmd {
        MagforceSub::Actuation { value } => {
            c.policy.require_writes()?;
            if c.policy.dry_run {
                c.note(format!("dry-run: MagForce actuation {value}"));
                return Ok(());
            }
            hid.set_actuation_point(*value)?;
            c.note(format!("MagForce actuation {value}"));
            Ok(())
        }
        MagforceSub::Setting {
            switch_mode,
            linear_mode,
            joystick,
        } => {
            c.policy.require_writes()?;
            if c.policy.dry_run {
                c.note("dry-run: MagForce key setting");
                return Ok(());
            }
            hid.set_key_setting(*switch_mode, *linear_mode, *joystick)?;
            c.note(format!(
                "MagForce switch={switch_mode} linear={linear_mode} joystick={joystick}"
            ));
            Ok(())
        }
        MagforceSub::Analog { mode } => {
            c.policy.require_writes()?;
            if c.policy.dry_run {
                c.note(format!("dry-run: MagForce analog curve {mode}"));
                return Ok(());
            }
            hid.set_analog_curve(*mode)?;
            c.note(format!("MagForce analog curve {mode}"));
            Ok(())
        }
        MagforceSub::Replace {
            state,
            backlight_30s,
        } => {
            c.policy.require_writes()?;
            if c.policy.dry_run {
                c.note("dry-run: MagForce replace mode");
                return Ok(());
            }
            hid.set_magforce_replace_mode(state.is_on(), *backlight_30s)?;
            c.note(format!(
                "MagForce replace={} 30s={}",
                state.is_on(),
                backlight_30s
            ));
            Ok(())
        }
        MagforceSub::Gamekeys { keys } => {
            if keys.len() != 4 {
                bail!("--keys needs exactly four hardware tags");
            }
            c.policy.require_writes()?;
            if c.policy.dry_run {
                c.note(format!("dry-run: MagForce game keys {keys:?}"));
                return Ok(());
            }
            let mut arr = [0u8; 4];
            arr.copy_from_slice(keys);
            hid.set_gamekeys(arr)?;
            c.note(format!("MagForce game keys {keys:?}"));
            Ok(())
        }
        MagforceSub::Status => {
            println!("key setting   {}", hexs(&hid.get_key_setting()?));
            println!("actuation     {}", hexs(&hid.get_actuation_point()?));
            println!("analog curve  {}", hexs(&hid.get_analog_curve()?));
            println!("30s backlight {}", hexs(&hid.get_backlight_30s()?));
            println!("game keys     {}", hexs(&hid.get_gamekeys()?));
            Ok(())
        }
    }
}

fn hid(c: &mut Controller, cmd: &HidSub) -> Result<()> {
    let hid = c.hid.as_ref().context("keyboard HID device unavailable")?;
    match cmd {
        HidSub::Get { opcode } => {
            let mut cmd = [0u8; 8];
            cmd[0] = *opcode;
            let resp = hid.raw_get(cmd)?;
            println!("command {}", hexs(&cmd));
            println!("response {}", hexs(&resp));
            Ok(())
        }
        HidSub::Raw { bytes } => {
            let v = parse_bytes(bytes)?;
            if v.len() != 8 {
                bail!("HID command must be exactly 8 bytes, got {}", v.len());
            }
            let mut cmd = [0u8; 8];
            cmd.copy_from_slice(&v);
            hid.raw_cmd(cmd)?;
            println!("sent {}", hexs(&cmd));
            Ok(())
        }
        HidSub::Status => {
            println!("health 0x88: {}", hexs(&hid.status()?));
            println!("status 0xB0: {}", hexs(&hid.status_b0()?));
            Ok(())
        }
    }
}

fn profile_cmd(c: &mut Controller, cmd: &ProfileSub) -> Result<()> {
    let store = ProfileStore::default();
    match cmd {
        ProfileSub::List => {
            for name in store.list()? {
                println!("{name}");
            }
            Ok(())
        }
        ProfileSub::Show { name } => {
            let p = store.load_or_default(name);
            print!("{}", toml::to_string_pretty(&p)?);
            Ok(())
        }
        ProfileSub::Save { name, from_file } => {
            let mut p = if let Some(path) = from_file {
                let text = std::fs::read_to_string(path)?;
                toml::from_str::<Profile>(&text)?
            } else {
                Profile::default()
            };
            p.name = name.clone();
            let path = store.save(&p)?;
            println!("{}", path.display());
            Ok(())
        }
        ProfileSub::Apply { name } => {
            let p = store.load_or_default(name);
            c.apply_profile(&p).map_err(Into::into)
        }
        ProfileSub::Delete { name } => {
            store.delete(name)?;
            println!("deleted {name}");
            Ok(())
        }
        ProfileSub::Export { path } => {
            let n = store.export(std::path::Path::new(path))?;
            println!("exported {n} profiles to {path}");
            Ok(())
        }
        ProfileSub::Import { path } => {
            let n = store.import(std::path::Path::new(path))?;
            println!("imported {n} profiles");
            Ok(())
        }
    }
}

fn parse_rgb(s: &str) -> Result<Rgb> {
    Rgb::from_hex(s).with_context(|| format!("invalid colour {s:?}; expected #RRGGBB"))
}

fn parse_bytes(items: &[String]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    for item in items {
        for part in item.split(',') {
            let part = part
                .trim()
                .trim_start_matches("0x")
                .trim_start_matches("0X");
            if part.is_empty() {
                continue;
            }
            let v = u8::from_str_radix(part, 16)
                .with_context(|| format!("invalid hex byte {part:?}"))?;
            out.push(v);
        }
    }
    Ok(out)
}

fn parse_tags(s: &str) -> Result<Vec<u8>> {
    s.split(',')
        .map(|v| {
            v.trim()
                .parse::<u8>()
                .with_context(|| format!("invalid key tag {v:?}"))
        })
        .collect()
}

fn install_hints() {
    let repo = std::env::var("ACER_LIGHTING_REPO").unwrap_or_else(|_| {
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()))
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "<repo>".into())
    });
    eprintln!("installation hints");
    eprintln!("  udev:   sudo cp packaging/99-predator-sense.rules /etc/udev/rules.d/");
    eprintln!("          sudo udevadm control --reload && sudo udevadm trigger");
    eprintln!("  module: sudo dkms install kmod/acer_gaming_probe/1.0 -k $(uname -r)  # or: cd kmod && sudo make load");
    eprintln!("  repo:   {repo}");
    let _ = safety::latest_path();
}
