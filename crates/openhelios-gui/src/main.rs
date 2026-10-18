mod keyboard;
mod settings;
mod tabs;
mod tray;

use std::collections::{HashSet, VecDeque};
use std::path::Path;
use std::process::Command;
use std::sync::mpsc::{channel, Receiver};
use std::thread;
use std::time::{Duration, Instant};

use eframe::egui;
use egui::{Color32, RichText};
use openhelios_core::lighting::LightingState;
use openhelios_core::monitor::{HistoryPoint, Monitor, Sample};
use openhelios_core::oc;
use openhelios_core::profile::{
    LightingProfile, LinuxProfile, MagForceProfile, Profile, ProfileStore,
};
use openhelios_core::safety::{self, SafetyPolicy};
use openhelios_core::Controller;
use winit::platform::x11::EventLoopBuilderExtX11;

use crate::settings::GuiSettings;
use crate::tray::{TrayCommand, TrayEvent};

#[derive(Clone, Debug)]
enum TrayState {
    Disabled,
    Starting,
    Ready,
    Unavailable(String),
}

pub const CYAN: Color32 = Color32::from_rgb(0, 212, 232);
pub const CYAN_DIM: Color32 = Color32::from_rgb(0, 125, 145);
pub const BG: Color32 = Color32::from_rgb(7, 11, 15);
pub const PANEL: Color32 = Color32::from_rgb(13, 19, 25);
pub const TEXT: Color32 = Color32::from_rgb(214, 232, 238);

/// OpenHelios dark cockpit theme: near-black panels, cyan accents.
fn setup_theme(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = BG;
    visuals.window_fill = PANEL;
    visuals.window_stroke = egui::Stroke::new(1.0, Color32::from_rgb(20, 60, 70));
    visuals.faint_bg_color = Color32::from_rgb(15, 23, 29);
    visuals.extreme_bg_color = Color32::from_rgb(4, 7, 9);
    visuals.override_text_color = Some(TEXT);
    visuals.selection.bg_fill = CYAN_DIM;
    visuals.selection.stroke = egui::Stroke::new(1.0, CYAN);
    visuals.hyperlink_color = CYAN;
    for (w, fill, stroke) in [
        (
            &mut visuals.widgets.inactive,
            Color32::from_rgb(18, 26, 33),
            Color32::from_rgb(28, 46, 55),
        ),
        (
            &mut visuals.widgets.hovered,
            Color32::from_rgb(24, 44, 53),
            CYAN_DIM,
        ),
        (&mut visuals.widgets.active, CYAN_DIM, CYAN),
    ] {
        w.bg_fill = fill;
        w.weak_bg_fill = fill;
        w.bg_stroke = egui::Stroke::new(1.0, stroke);
        w.fg_stroke = egui::Stroke::new(1.0, TEXT);
    }
    visuals.widgets.noninteractive.bg_fill = PANEL;
    visuals.widgets.noninteractive.bg_stroke =
        egui::Stroke::new(1.0, Color32::from_rgb(22, 40, 48));
    visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, TEXT);
    for w in [
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
        &mut visuals.widgets.noninteractive,
    ] {
        w.corner_radius = egui::CornerRadius::same(3);
    }
    ctx.all_styles_mut(|style| {
        style.visuals = visuals.clone();
        style.spacing.item_spacing = egui::vec2(9.0, 8.0);
        style.spacing.button_padding = egui::vec2(10.0, 6.0);
        style
            .text_styles
            .insert(egui::TextStyle::Heading, egui::FontId::proportional(25.0));
        style
            .text_styles
            .insert(egui::TextStyle::Body, egui::FontId::proportional(14.0));
        style
            .text_styles
            .insert(egui::TextStyle::Button, egui::FontId::proportional(14.0));
        style
            .text_styles
            .insert(egui::TextStyle::Small, egui::FontId::proportional(11.5));
    });
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    Dashboard,
    Lighting,
    Fans,
    Overclock,
    MagForce,
    Power,
    Profiles,
    Diagnostics,
    Recovery,
}

impl Tab {
    pub fn label(self) -> &'static str {
        match self {
            Tab::Dashboard => "Home",
            Tab::Lighting => "Lighting",
            Tab::Overclock => "Overclocking",
            Tab::Fans => "Fan Control",
            Tab::Power => "Power / GPU",
            Tab::MagForce => "MagForce",
            Tab::Profiles => "Profiles",
            Tab::Diagnostics => "Diagnostics",
            Tab::Recovery => "Recovery",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.to_ascii_lowercase().as_str() {
            "dashboard" => Tab::Dashboard,
            "lighting" => Tab::Lighting,
            "fans" | "fan" => Tab::Fans,
            "overclock" | "oc" => Tab::Overclock,
            "magforce" | "mag" => Tab::MagForce,
            "power" | "gpu" => Tab::Power,
            "profiles" => Tab::Profiles,
            "diagnostics" | "diag" => Tab::Diagnostics,
            "recovery" => Tab::Recovery,
            _ => return None,
        })
    }
}

pub struct App {
    pub controller: Controller,
    monitor_rx: Receiver<Sample>,
    pub sample: Sample,
    pub history: VecDeque<HistoryPoint>,
    pub profile: Profile,
    pub state: LightingState,
    pub store: ProfileStore,
    pub tab: Tab,
    pub selected: HashSet<u8>,
    pub paint: [u8; 3],
    pub status: String,
    pub read_only: bool,
    pub dry_run: bool,
    pub allow_oc: bool,
    self_test: bool,
    frames: u64,
    screenshot_path: Option<String>,
    screenshot_requested: bool,
    screenshot_mode: bool,
    pub profile_name: String,
    pub raw_method: u8,
    pub raw_payload: String,
    pub raw_response: String,
    pub hid_opcode: u8,
    pub hid_response: String,
    pub export_path: String,
    pub import_path: String,
    pub linux_governor: String,
    pub linux_epp: String,
    pub linux_no_turbo: bool,
    pub linux_max_freq_khz: u64,
    pub fan_status: String,
    pub mag_actuation: u8,
    pub mag_switch: u8,
    pub mag_linear: u8,
    pub mag_joystick: bool,
    pub mag_analog: u8,
    pub mag_replace: bool,
    pub mag_replace_30s: bool,
    pub mag_gamekeys: [u8; 4],
    pub mag_status: String,
    pub pl1_w: u32,
    pub pl2_w: u32,
    pub gpu_power_w: u32,
    pub gpu_lock_min: f32,
    pub gpu_lock_max: f32,
    pub power_limits: oc::PowerLimits,
    pub nvidia_power: Option<oc::NvidiaPower>,
    pub auto_turbo: bool,
    last_slide: Option<bool>,
    pub turbo_active: bool,
    turbo_backup: Option<Profile>,
    last_saved: String,
    last_persist: Instant,
    settings: GuiSettings,
    tray_rx: Receiver<TrayEvent>,
    tray_state: TrayState,
    hidden: bool,
    /// winit cannot hide a native Wayland window; without X11 "hide" minimizes.
    can_hide: bool,
    /// eframe shows the window after its first paint; override that for a few
    /// immediate frames so a tray start does not flash the window.
    hide_frames: u8,
    started_at: Instant,
    quitting: bool,
}

impl App {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        self_test: bool,
        initial_tab: Tab,
        screenshot_path: Option<String>,
        start_hidden: bool,
        can_hide: bool,
    ) -> Self {
        setup_theme(&cc.egui_ctx);
        let screenshot_path_is_set = screenshot_path.is_some();
        let settings = GuiSettings::load();
        let mut status = "ready".to_string();
        let (tray_rx, tray_state) = if self_test || screenshot_path_is_set {
            let (_tx, rx) = channel();
            (rx, TrayState::Disabled)
        } else {
            (tray::spawn(cc.egui_ctx.clone()), TrayState::Starting)
        };
        if !self_test && !screenshot_path_is_set {
            if let Err(e) = settings.sync_autostart() {
                status = format!("autostart: {e}");
            }
        }
        let policy = SafetyPolicy::default();
        let controller = Controller::detect(policy);
        let store = ProfileStore::default();
        let profile = store.load_or_default("default");
        let state = profile.lighting.state();
        let paint = state.color.to_array();
        let magforce = profile.magforce.clone();
        let linux = profile.linux.clone();
        let auto_turbo = profile.auto_turbo;
        let last_saved = toml::to_string_pretty(&profile).unwrap_or_default();

        let (tx, monitor_rx) = channel();
        thread::spawn(move || {
            let mut monitor = Monitor::new();
            loop {
                let sample = monitor.sample();
                if tx.send(sample).is_err() {
                    break;
                }
                thread::sleep(Duration::from_secs(1));
            }
        });

        let cpu = oc::cpu_status();
        let linux_governor = linux.governor.clone();
        let linux_epp = linux.epp.clone();
        let linux_no_turbo = linux.no_turbo;
        let linux_max_freq_khz = linux.max_freq_khz.or(cpu.max_freq_khz).unwrap_or(0);
        let limits = oc::PowerLimits::read();
        let nv = oc::nvidia_status();

        Self {
            controller,
            monitor_rx,
            sample: Sample::default(),
            history: VecDeque::with_capacity(180),
            profile,
            state,
            store,
            tab: initial_tab,
            selected: HashSet::new(),
            paint,
            status,
            read_only: false,
            dry_run: false,
            allow_oc: false,
            self_test,
            frames: 0,
            screenshot_path,
            screenshot_requested: false,
            screenshot_mode: screenshot_path_is_set,
            profile_name: "default".to_string(),
            raw_method: 5,
            raw_payload: "00 00 00 00 00 00 00 00".to_string(),
            raw_response: String::new(),
            hid_opcode: 0x88,
            hid_response: String::new(),
            export_path: "openhelios-profiles.json".to_string(),
            import_path: "openhelios-profiles.json".to_string(),
            linux_governor,
            linux_epp,
            linux_no_turbo,
            linux_max_freq_khz,
            fan_status: String::new(),
            mag_actuation: magforce.actuation,
            mag_switch: magforce.switch_mode,
            mag_linear: magforce.linear_mode,
            mag_joystick: magforce.joystick,
            mag_analog: magforce.analog_curve,
            mag_replace: magforce.replace_mode,
            mag_replace_30s: magforce.replace_30s,
            mag_gamekeys: magforce.gamekeys,
            mag_status: String::new(),
            pl1_w: limits.pl1_uw.map(|v| (v / 1_000_000) as u32).unwrap_or(45),
            pl2_w: limits.pl2_uw.map(|v| (v / 1_000_000) as u32).unwrap_or(56),
            gpu_power_w: nv
                .as_ref()
                .and_then(|n| n.power_limit_w)
                .unwrap_or_else(|| nv.as_ref().and_then(|n| n.max_power_w).unwrap_or(115.0))
                as u32,
            gpu_lock_min: nv
                .as_ref()
                .and_then(|n| n.current_clock_mhz)
                .unwrap_or(300.0),
            gpu_lock_max: nv.as_ref().and_then(|n| n.max_clock_mhz).unwrap_or(2100.0),
            power_limits: limits,
            nvidia_power: nv,
            auto_turbo,
            last_slide: None,
            turbo_active: false,
            turbo_backup: None,
            last_saved,
            last_persist: Instant::now(),
            settings,
            tray_rx,
            tray_state,
            hidden: start_hidden,
            can_hide,
            hide_frames: if start_hidden { 8 } else { 0 },
            started_at: Instant::now(),
            quitting: false,
        }
    }

    pub fn current_profile(&self) -> Profile {
        let mut p = self.profile.clone();
        p.lighting = LightingProfile::from_state(&self.state);
        p.magforce = MagForceProfile {
            actuation: self.mag_actuation,
            switch_mode: self.mag_switch,
            linear_mode: self.mag_linear,
            joystick: self.mag_joystick,
            analog_curve: self.mag_analog,
            replace_mode: self.mag_replace,
            replace_30s: self.mag_replace_30s,
            gamekeys: self.mag_gamekeys,
        };
        p.linux = LinuxProfile {
            governor: self.linux_governor.clone(),
            epp: self.linux_epp.clone(),
            no_turbo: self.linux_no_turbo,
            max_freq_khz: (self.linux_max_freq_khz > 0).then_some(self.linux_max_freq_khz),
        };
        p.auto_turbo = self.auto_turbo;
        p.name = self.profile_name.clone();
        p.validate();
        p
    }

    fn persist_settings(&mut self) {
        let saved = self.settings.save();
        let autostart = self.settings.sync_autostart();
        self.status = match (saved, autostart) {
            (Err(e), _) => format!("settings: {e}"),
            (_, Err(e)) => format!("autostart: {e}"),
            _ if self.settings.start_with_system => "start at login updated".to_string(),
            _ => "start at login disabled".to_string(),
        };
    }

    fn drain_tray(&mut self, ctx: &egui::Context) {
        while let Ok(event) = self.tray_rx.try_recv() {
            match event {
                TrayEvent::Ready => self.tray_state = TrayState::Ready,
                TrayEvent::Unavailable(error) => {
                    self.tray_state = TrayState::Unavailable(error.clone());
                    if self.hidden {
                        self.show_window(ctx);
                    }
                    self.status = format!("tray unavailable: {error}");
                }
                TrayEvent::Command(TrayCommand::Show) => self.show_window(ctx),
                TrayEvent::Command(TrayCommand::Hide) => self.hide_window(ctx),
                TrayEvent::Command(TrayCommand::Toggle) => {
                    if self.hidden {
                        self.show_window(ctx);
                    } else {
                        self.hide_window(ctx);
                    }
                }
                TrayEvent::Command(TrayCommand::Quit) => self.request_quit(ctx),
            }
        }
    }

    fn show_window(&mut self, ctx: &egui::Context) {
        self.hidden = false;
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    }

    fn hide_window(&mut self, ctx: &egui::Context) {
        self.hidden = true;
        if self.can_hide {
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        } else {
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
        }
    }

    fn request_quit(&mut self, ctx: &egui::Context) {
        self.quitting = true;
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }

    fn update_engine(&mut self) {
        let slide = self.sample.slide_keyboard;
        if slide != self.last_slide {
            self.last_slide = slide;
            match slide {
                Some(false) if self.turbo_active => self.release_turbo(),
                Some(true) if self.auto_turbo && !self.read_only && !self.turbo_active => {
                    self.engage_turbo(true);
                }
                Some(true) if !self.auto_turbo => {
                    self.status = "keyboard slide engaged (Auto Turbo is off)".to_string();
                }
                _ => {}
            }
        }

        if self.last_persist.elapsed() >= Duration::from_secs(1) {
            let profile = self.current_profile();
            if let Ok(text) = toml::to_string_pretty(&profile) {
                if text != self.last_saved {
                    match self.store.save(&profile) {
                        Ok(path) => {
                            self.last_saved = text;
                            self.status = format!("saved {}", path.display());
                        }
                        Err(e) => self.status = format!("autosave failed: {e}"),
                    }
                }
            }
            self.last_persist = Instant::now();
        }
    }

    pub fn snapshot_now(&mut self) {
        if !self.controller.policy.snapshot_before_write || self.dry_run {
            return;
        }
        let p = self.current_profile();
        match self.controller.save_state(&p) {
            Ok(path) => self.status = format!("snapshot: {}", path.display()),
            Err(e) => self.status = format!("snapshot failed: {e}"),
        }
    }

    pub fn report(&mut self, r: openhelios_core::Result<()>) {
        match r {
            Ok(()) => {
                self.status = self
                    .controller
                    .log
                    .last()
                    .cloned()
                    .unwrap_or_else(|| "ok".to_string());
            }
            Err(e) => self.status = format!("error: {e}"),
        }
    }

    pub fn report_msg(&mut self, r: std::result::Result<(), String>) {
        match r {
            Ok(()) => self.status = "ok".to_string(),
            Err(e) => self.status = format!("error: {e}"),
        }
    }

    pub(crate) fn run_privileged(&self, args: &[&str]) -> std::result::Result<(), String> {
        let cli = if Path::new("/usr/local/bin/openhelios").exists() {
            "/usr/local/bin/openhelios"
        } else {
            "openhelios"
        };
        let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
        match Command::new("sudo")
            .arg("-n")
            .arg("env")
            .arg(format!("HOME={home}"))
            .arg(cli)
            .args(args)
            .output()
        {
            Ok(out) if out.status.success() => return Ok(()),
            Ok(out) => {
                let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
                if !err.to_ascii_lowercase().contains("password") {
                    return Err(if err.is_empty() {
                        "administrator command failed".to_string()
                    } else {
                        err
                    });
                }
            }
            Err(e) => return Err(format!("sudo: {e}")),
        }
        match Command::new("pkexec").arg(cli).args(args).output() {
            Ok(out) if out.status.success() => Ok(()),
            Ok(out) => {
                let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
                Err(if err.is_empty() {
                    "pkexec failed".to_string()
                } else {
                    err
                })
            }
            Err(e) => Err(format!("pkexec: {e}")),
        }
    }

    pub(crate) fn engage_turbo(&mut self, from_slide: bool) {
        if !from_slide && !self.allow_oc {
            self.status = "Turbo needs the allow OC switch".to_string();
            return;
        }
        if self.sample.ac_online != Some(true) {
            self.status = "Turbo needs AC power".to_string();
            return;
        }
        if let Some(pct) = self.sample.battery_percent {
            if pct < 40.0 {
                self.status = format!("Turbo needs at least 40% battery (have {pct:.0}%)");
                return;
            }
        }
        self.turbo_backup = Some(self.current_profile());
        self.snapshot_now();
        if let Err(e) = self.run_privileged(&["--allow-oc", "turbo", "apply", "--oc-only"]) {
            self.status = format!("Turbo privileged apply failed: {e}");
            return;
        }
        if let Err(e) = self.controller.apply_turbo_visuals() {
            self.status = format!("Turbo lights/fans failed: {e}");
            return;
        }
        self.turbo_active = true;
        self.status =
            "TURBO engaged: fans 100%, heat-pipe/fan/ring lights on, orange keyboard wave"
                .to_string();
    }

    pub(crate) fn release_turbo(&mut self) {
        if let Err(e) = self.run_privileged(&["--allow-oc", "turbo", "restore", "--oc-only"]) {
            self.status = format!("Turbo privileged restore failed: {e}");
        }
        if let Some(backup) = self.turbo_backup.take() {
            if let Err(e) = self.controller.apply_visuals(&backup) {
                self.status = format!("Turbo visual restore failed: {e}");
            }
        }
        self.turbo_active = false;
        if !self.status.starts_with("Turbo privileged") {
            self.status = "Turbo released; saved profile restored".to_string();
        }
    }

    pub(crate) fn sync_profile_fields(&mut self) {
        self.mag_actuation = self.profile.magforce.actuation;
        self.mag_switch = self.profile.magforce.switch_mode;
        self.mag_linear = self.profile.magforce.linear_mode;
        self.mag_joystick = self.profile.magforce.joystick;
        self.mag_analog = self.profile.magforce.analog_curve;
        self.mag_replace = self.profile.magforce.replace_mode;
        self.mag_replace_30s = self.profile.magforce.replace_30s;
        self.mag_gamekeys = self.profile.magforce.gamekeys;
        self.linux_governor = self.profile.linux.governor.clone();
        self.linux_epp = self.profile.linux.epp.clone();
        self.linux_no_turbo = self.profile.linux.no_turbo;
        self.linux_max_freq_khz = self.profile.linux.max_freq_khz.unwrap_or(0);
        self.auto_turbo = self.profile.auto_turbo;
        self.sync_power_fields();
        self.last_saved = toml::to_string_pretty(&self.profile).unwrap_or_default();
        self.last_persist = Instant::now();
    }

    fn drain_monitor(&mut self) {
        while let Ok(s) = self.monitor_rx.try_recv() {
            self.history.push_back(HistoryPoint {
                cpu_temp: s.cpu_temp.unwrap_or(0.0),
                gpu_temp: s.gpu_temp.unwrap_or(0.0),
                cpu_load: s.cpu_load.unwrap_or(0.0),
                gpu_load: s.gpu_load.unwrap_or(0.0),
            });
            while self.history.len() > 180 {
                self.history.pop_front();
            }
            self.sample = s;
        }
    }

    fn top_panel(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("top")
            .frame(
                egui::Frame::side_top_panel(ui.style())
                    .fill(PANEL)
                    .inner_margin(egui::Margin::symmetric(16, 7)),
            )
            .show(ui, |ui| {
                let width = ui.available_width();
                if width < 900.0 {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("OpenHelios").size(20.0).strong().color(TEXT));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.checkbox(&mut self.allow_oc, "OC");
                            ui.checkbox(&mut self.dry_run, "dry");
                            ui.checkbox(&mut self.read_only, "RO");
                        });
                    });
                    let rect = ui.max_rect();
                    ui.painter().line_segment(
                        [rect.left_bottom(), rect.right_bottom()],
                        egui::Stroke::new(1.0, Color32::from_rgb(20, 70, 82)),
                    );
                } else {
                    ui.columns(3, |cols| {
                        cols[0].with_layout(
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                ui.label(
                                    RichText::new("OPENHELIOS").size(15.0).strong().color(CYAN),
                                );
                                ui.label(
                                    RichText::new(format!(
                                        "{} / {}",
                                        self.controller.status.model, self.controller.status.board
                                    ))
                                    .small()
                                    .color(Color32::from_gray(150)),
                                );
                                let wmi = if self.controller.status.wmi {
                                    RichText::new("WMI")
                                        .small()
                                        .color(Color32::from_rgb(120, 230, 140))
                                } else {
                                    RichText::new("WMI")
                                        .small()
                                        .color(Color32::from_rgb(255, 120, 120))
                                };
                                let hid = if self.controller.status.hid {
                                    RichText::new("HID")
                                        .small()
                                        .color(Color32::from_rgb(120, 230, 140))
                                } else {
                                    RichText::new("HID")
                                        .small()
                                        .color(Color32::from_rgb(255, 120, 120))
                                };
                                ui.label(wmi);
                                ui.label(hid);
                            },
                        );
                        cols[1].vertical_centered(|ui| {
                            ui.label(RichText::new("OpenHelios").size(22.0).strong().color(TEXT));
                        });
                        cols[2].with_layout(
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| {
                                ui.checkbox(&mut self.allow_oc, "allow OC");
                                ui.checkbox(&mut self.dry_run, "dry-run");
                                ui.checkbox(&mut self.read_only, "read-only");
                                let (slide, color) = match self.sample.slide_keyboard {
                                    Some(true) => ("SLIDE", Color32::from_rgb(255, 140, 60)),
                                    Some(false) => ("slide closed", Color32::from_gray(150)),
                                    None => ("slide ?", Color32::from_gray(110)),
                                };
                                ui.label(RichText::new(slide).small().strong().color(color));
                            },
                        );
                    });
                    let rect = ui.max_rect();
                    ui.painter().line_segment(
                        [rect.left_bottom(), rect.right_bottom()],
                        egui::Stroke::new(1.0, Color32::from_rgb(20, 70, 82)),
                    );
                }
            });
        self.controller.policy.allow_writes = !self.read_only;
        self.controller.policy.allow_oc = self.allow_oc;
        self.controller.policy.dry_run = self.dry_run;
    }

    fn side_panel(&mut self, ui: &mut egui::Ui) {
        egui::Panel::left("nav")
            .resizable(false)
            .exact_size(198.0)
            .frame(
                egui::Frame::side_top_panel(ui.style())
                    .fill(BG)
                    .inner_margin(egui::Margin::symmetric(12, 10)),
            )
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
                    .show(ui, |ui| {
                        ui.add_space(6.0);
                        ui.label(RichText::new("OPENHELIOS").size(22.0).strong().color(CYAN));
                        ui.label(
                            RichText::new("LINUX CONTROL")
                                .size(10.5)
                                .color(Color32::from_gray(150)),
                        );
                        ui.add_space(6.0);
                        ui.separator();
                        ui.add_space(10.0);
                        for tab in [
                            Tab::Dashboard,
                            Tab::Lighting,
                            Tab::Overclock,
                            Tab::Fans,
                            Tab::Power,
                            Tab::MagForce,
                            Tab::Profiles,
                            Tab::Diagnostics,
                            Tab::Recovery,
                        ] {
                            let selected = self.tab == tab;
                            let fill = if selected {
                                CYAN
                            } else {
                                Color32::from_rgb(12, 18, 24)
                            };
                            let text = if selected { Color32::BLACK } else { TEXT };
                            let stroke = egui::Stroke::new(
                                1.0,
                                if selected {
                                    CYAN
                                } else {
                                    Color32::from_rgb(24, 56, 66)
                                },
                            );
                            let button = egui::Button::new(
                                RichText::new(tab.label()).size(14.5).color(text),
                            )
                            .fill(fill)
                            .stroke(stroke)
                            .corner_radius(2.0)
                            .min_size(egui::vec2(ui.available_width(), 34.0));
                            if ui.add(button).clicked() {
                                self.tab = tab;
                            }
                            ui.add_space(3.0);
                        }
                        ui.add_space(12.0);
                        ui.separator();
                        ui.label(
                            RichText::new("SAFETY")
                                .small()
                                .color(Color32::from_gray(130)),
                        );
                        ui.label(if self.read_only {
                            RichText::new("read-only").color(Color32::from_rgb(255, 200, 90))
                        } else {
                            RichText::new("writes enabled").color(Color32::from_rgb(120, 230, 140))
                        });
                        ui.label(if self.allow_oc {
                            RichText::new("EC OC enabled").color(Color32::from_rgb(255, 120, 90))
                        } else {
                            RichText::new("EC OC locked").color(Color32::from_gray(160))
                        });
                        if self.dry_run {
                            ui.label(
                                RichText::new("dry-run").color(Color32::from_rgb(120, 190, 255)),
                            );
                        }
                        ui.add_space(10.0);
                        ui.separator();
                        ui.label(
                            RichText::new("SYSTEM")
                                .small()
                                .color(Color32::from_gray(130)),
                        );
                        let mut start_with_system = self.settings.start_with_system;
                        if ui
                            .checkbox(&mut start_with_system, "start at login")
                            .changed()
                        {
                            self.settings.start_with_system = start_with_system;
                            self.persist_settings();
                        }
                        let mut start_in_tray = self.settings.start_in_tray;
                        if ui.checkbox(&mut start_in_tray, "start in tray").changed() {
                            self.settings.start_in_tray = start_in_tray;
                            self.persist_settings();
                        }
                        let mut close_to_tray = self.settings.close_to_tray;
                        if ui.checkbox(&mut close_to_tray, "close to tray").changed() {
                            self.settings.close_to_tray = close_to_tray;
                            self.persist_settings();
                        }
                        match &self.tray_state {
                            TrayState::Disabled => ui.label(
                                RichText::new("tray: disabled")
                                    .small()
                                    .color(Color32::from_gray(120)),
                            ),
                            TrayState::Starting => ui.label(
                                RichText::new("tray: starting")
                                    .small()
                                    .color(Color32::from_gray(150)),
                            ),
                            TrayState::Ready => ui.label(
                                RichText::new("tray: ready")
                                    .small()
                                    .color(Color32::from_rgb(120, 230, 140)),
                            ),
                            TrayState::Unavailable(error) => ui.label(
                                RichText::new(format!("tray: {error}"))
                                    .small()
                                    .color(Color32::from_rgb(255, 120, 120)),
                            ),
                        };
                        if ui.button("Quit OpenHelios").clicked() {
                            self.request_quit(ui.ctx());
                        }
                        ui.add_space(10.0);
                        ui.separator();
                        ui.label(
                            RichText::new("SNAPSHOT")
                                .small()
                                .color(Color32::from_gray(130)),
                        );
                        let latest = safety::latest_path();
                        let latest_text = if latest.exists() {
                            latest.display().to_string()
                        } else {
                            "none — press Snapshot".to_string()
                        };
                        ui.add(
                            egui::Label::new(
                                RichText::new(latest_text)
                                    .small()
                                    .color(Color32::from_gray(150)),
                            )
                            .truncate(),
                        );
                        ui.add_space(12.0);
                        ui.label(
                            RichText::new(env!("CARGO_PKG_VERSION"))
                                .small()
                                .color(Color32::from_gray(100)),
                        );
                        ui.label(
                            RichText::new("OpenHelios / PH717")
                                .small()
                                .color(Color32::from_gray(100)),
                        );
                    });
            });
    }

    fn bottom_panel(&mut self, ui: &mut egui::Ui) {
        egui::Panel::bottom("status")
            .frame(
                egui::Frame::side_top_panel(ui.style())
                    .fill(Color32::from_rgb(18, 19, 23))
                    .inner_margin(egui::Margin::symmetric(14, 6)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.add(
                        egui::Label::new(
                            RichText::new(&self.status).color(Color32::from_rgb(200, 220, 255)),
                        )
                        .truncate(),
                    );
                    if ui.available_width() < 620.0 {
                        return;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if let Some(t) = self.sample.cpu_temp {
                            ui.label(format!("CPU {t:.0}°C"));
                        }
                        if let Some(t) = self.sample.gpu_temp {
                            ui.label(format!("GPU {t:.0}°C"));
                        }
                        if let Some(b) = self.sample.battery_percent {
                            ui.label(format!("battery {b:.0}%"));
                        }
                        ui.label(if self.sample.ac_online == Some(true) {
                            RichText::new("AC").color(Color32::from_rgb(120, 230, 140))
                        } else {
                            RichText::new("battery").color(Color32::from_rgb(255, 200, 90))
                        });
                    });
                });
            });
    }
}

impl Drop for App {
    fn drop(&mut self) {
        if self.turbo_active {
            let _ = self.run_privileged(&["--allow-oc", "turbo", "restore", "--oc-only"]);
            if let Some(backup) = self.turbo_backup.take() {
                let _ = self.controller.apply_visuals(&backup);
            }
        }
    }
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.request_repaint_after(Duration::from_millis(250));
        if self.self_test {
            ctx.request_repaint();
        }

        self.drain_tray(ctx);

        if self.hidden
            && matches!(self.tray_state, TrayState::Starting)
            && self.started_at.elapsed() >= Duration::from_secs(3)
        {
            self.show_window(ctx);
            self.status = "tray did not start; showing the window".to_string();
        }

        if ctx.input(|i| i.viewport().close_requested())
            && !self.quitting
            && self.settings.close_to_tray
            && matches!(self.tray_state, TrayState::Starting | TrayState::Ready)
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.hide_window(ctx);
            self.status = if self.can_hide {
                "running in the system tray".to_string()
            } else {
                "minimized: hiding needs XWayland, which this session lacks".to_string()
            };
        }

        self.drain_monitor();
        self.update_engine();

        if self.hidden && self.can_hide {
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            if self.hide_frames > 0 {
                self.hide_frames -= 1;
                ctx.request_repaint();
            }
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if self.self_test {
            self.tab = match self.frames % 9 {
                0 => Tab::Dashboard,
                1 => Tab::Lighting,
                2 => Tab::Fans,
                3 => Tab::Overclock,
                4 => Tab::MagForce,
                5 => Tab::Power,
                6 => Tab::Profiles,
                7 => Tab::Diagnostics,
                _ => Tab::Recovery,
            };
        }
        self.top_panel(ui);
        self.side_panel(ui);
        self.bottom_panel(ui);

        egui::CentralPanel::default().show(ui, |ui| {
            let rect = ui.max_rect();
            {
                let painter = ui.painter();
                let grid = egui::Stroke::new(1.0, Color32::from_rgb(13, 21, 27));
                let mut y = rect.top();
                while y < rect.bottom() {
                    painter.line_segment(
                        [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
                        grid,
                    );
                    y += 26.0;
                }
                painter.rect_stroke(
                    rect.shrink(6.0),
                    10.0,
                    egui::Stroke::new(1.0, Color32::from_rgb(18, 58, 68)),
                    egui::StrokeKind::Inside,
                );
            }
            // 16 px gutter keeps every control and heading inside the frame.
            egui::Frame::NONE
                .inner_margin(egui::Margin::symmetric(16, 10))
                .show(ui, |ui| {
                    egui::ScrollArea::horizontal()
                        .auto_shrink([false, false])
                        .scroll_bar_visibility(
                            egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded,
                        )
                        .show(ui, |ui| {
                            ui.set_min_width(560.0);
                            egui::ScrollArea::vertical()
                                .auto_shrink([false, false])
                                .scroll_bar_visibility(
                                    egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded,
                                )
                                .show(ui, |ui| match self.tab {
                                    Tab::Dashboard => self.dashboard(ui),
                                    Tab::Lighting => self.lighting_tab(ui),
                                    Tab::Fans => self.fans_tab(ui),
                                    Tab::Overclock => self.overclock_tab(ui),
                                    Tab::MagForce => self.magforce_tab(ui),
                                    Tab::Power => self.power_tab(ui),
                                    Tab::Profiles => self.profiles_tab(ui),
                                    Tab::Diagnostics => self.diagnostics_tab(ui),
                                    Tab::Recovery => self.recovery_tab(ui),
                                });
                        });
                });
        });

        if let Some(path) = self.screenshot_path.clone() {
            self.frames += 1;
            if self.frames >= 90 && !self.screenshot_requested {
                self.screenshot_requested = true;
                let ctx = ui.ctx().clone();
                ui.ctx().request_screenshot(move |image| {
                    let width = image.size[0] as u32;
                    let height = image.size[1] as u32;
                    let mut bytes = Vec::with_capacity(image.pixels.len() * 4);
                    for p in &image.pixels {
                        bytes.extend_from_slice(&p.to_array());
                    }
                    if let Err(e) =
                        image::save_buffer(&path, &bytes, width, height, image::ColorType::Rgba8)
                    {
                        eprintln!("openhelios-gui: screenshot failed: {e}");
                    }
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                });
            }
            ui.ctx().request_repaint();
        }

        if self.self_test && self.screenshot_path.is_none() {
            self.frames += 1;
            if self.frames > 10 {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }
}

fn main() -> eframe::Result<()> {
    let mut self_test = false;
    let mut initial_tab = Tab::Dashboard;
    let mut screenshot_path = None;
    let mut start_hidden = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--self-test" => self_test = true,
            "--minimized" | "--tray" => start_hidden = true,
            "--tab" => {
                if let Some(value) = args.next() {
                    if let Some(tab) = Tab::parse(&value) {
                        initial_tab = tab;
                    }
                }
            }
            "--screenshot" => {
                screenshot_path = args.next();
            }
            _ => {}
        }
    }
    // winit's Wayland backend ignores `set_visible`, so a native Wayland window
    // can never be hidden to the tray. Run on X11 (XWayland inside a Wayland
    // session) whenever an X display is available.
    let can_hide = std::env::var_os("DISPLAY").is_some_and(|display| !display.is_empty());
    let start_hidden = start_hidden && can_hide && !self_test && screenshot_path.is_none();
    let inner_size = if screenshot_path.is_some() {
        [1500.0, 1000.0]
    } else {
        [1320.0, 900.0]
    };
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size(inner_size)
        .with_min_inner_size([800.0, 600.0])
        .with_title("OpenHelios")
        .with_visible(!start_hidden);
    if let Some(icon) = window_icon() {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions {
        viewport,
        event_loop_builder: can_hide.then(|| -> eframe::EventLoopBuilderHook {
            Box::new(|builder| {
                builder.with_x11();
            })
        }),
        ..Default::default()
    };
    eframe::run_native(
        "OpenHelios",
        options,
        Box::new(move |cc| {
            Ok(Box::new(App::new(
                cc,
                self_test,
                initial_tab,
                screenshot_path,
                start_hidden,
                can_hide,
            )))
        }),
    )
}

fn window_icon() -> Option<egui::IconData> {
    let image = image::load_from_memory(include_bytes!("../../../assets/logo.png")).ok()?;
    let image = image.into_rgba8();
    let (width, height) = image.dimensions();
    Some(egui::IconData {
        rgba: image.into_raw(),
        width,
        height,
    })
}
