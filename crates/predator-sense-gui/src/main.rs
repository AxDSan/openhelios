mod keyboard;
mod tabs;

use std::collections::{HashSet, VecDeque};
use std::sync::mpsc::{channel, Receiver};
use std::thread;
use std::time::Duration;

use eframe::egui;
use egui::{Color32, RichText};
use predator_core::lighting::LightingState;
use predator_core::monitor::{HistoryPoint, Monitor, Sample};
use predator_core::oc;
use predator_core::profile::{LightingProfile, Profile, ProfileStore};
use predator_core::safety::{self, SafetyPolicy};
use predator_core::Controller;

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
}

impl App {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        self_test: bool,
        initial_tab: Tab,
        screenshot_path: Option<String>,
    ) -> Self {
        cc.egui_ctx.set_visuals(egui::Visuals::dark());
        let screenshot_path_is_set = screenshot_path.is_some();
        let policy = SafetyPolicy::default();
        let controller = Controller::detect(policy);
        let store = ProfileStore::default();
        let profile = store.load_or_default("default");
        let state = profile.lighting.state();
        let paint = state.color.to_array();

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
        let linux_governor = cpu
            .governor
            .clone()
            .unwrap_or_else(|| "powersave".to_string());
        let linux_epp = cpu
            .epp
            .clone()
            .unwrap_or_else(|| "balance_performance".to_string());
        let linux_no_turbo = cpu.no_turbo.unwrap_or(false);
        let linux_max_freq_khz = cpu.max_freq_khz.unwrap_or(0);
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
            status: "ready".to_string(),
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
            export_path: "predator-sense-profiles.json".to_string(),
            import_path: "predator-sense-profiles.json".to_string(),
            linux_governor,
            linux_epp,
            linux_no_turbo,
            linux_max_freq_khz,
            fan_status: String::new(),
            mag_actuation: 46,
            mag_switch: 0,
            mag_linear: 0,
            mag_joystick: false,
            mag_analog: 0,
            mag_replace: false,
            mag_replace_30s: false,
            mag_gamekeys: [0, 1, 2, 3],
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
        }
    }

    pub fn current_profile(&self) -> Profile {
        let mut p = self.profile.clone();
        p.lighting = LightingProfile::from_state(&self.state);
        p.name = self.profile_name.clone();
        p.validate();
        p
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

    pub fn report(&mut self, r: predator_core::Result<()>) {
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
                    .fill(Color32::from_rgb(18, 19, 23))
                    .inner_margin(egui::Margin::symmetric(14, 8)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("PREDATORSENSE LINUX").strong().size(16.0));
                    ui.label(
                        RichText::new(format!(
                            "{} / {}",
                            self.controller.status.model, self.controller.status.board
                        ))
                        .color(Color32::from_gray(150)),
                    );
                    ui.separator();
                    let wmi = if self.controller.status.wmi {
                        RichText::new("WMI ok").color(Color32::from_rgb(120, 230, 140))
                    } else {
                        RichText::new("WMI missing").color(Color32::from_rgb(255, 120, 120))
                    };
                    ui.label(wmi);
                    let hid = if self.controller.status.hid {
                        RichText::new("HID ok").color(Color32::from_rgb(120, 230, 140))
                    } else {
                        RichText::new("HID missing").color(Color32::from_rgb(255, 120, 120))
                    };
                    ui.label(hid);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.checkbox(&mut self.allow_oc, "allow OC");
                        ui.checkbox(&mut self.dry_run, "dry-run");
                        ui.checkbox(&mut self.read_only, "read-only");
                    });
                });
            });
        self.controller.policy.allow_writes = !self.read_only;
        self.controller.policy.allow_oc = self.allow_oc;
        self.controller.policy.dry_run = self.dry_run;
    }

    fn side_panel(&mut self, ui: &mut egui::Ui) {
        egui::Panel::left("nav")
            .resizable(false)
            .exact_size(186.0)
            .frame(
                egui::Frame::side_top_panel(ui.style())
                    .fill(Color32::from_rgb(15, 16, 20))
                    .inner_margin(egui::Margin::same(10)),
            )
            .show(ui, |ui| {
                ui.add_space(4.0);
                ui.label(
                    RichText::new("CONTROL")
                        .small()
                        .color(Color32::from_gray(130)),
                );
                ui.add_space(6.0);
                for (tab, label) in [
                    (Tab::Dashboard, "Dashboard"),
                    (Tab::Lighting, "Lighting"),
                    (Tab::Fans, "Fan control"),
                    (Tab::Overclock, "Overclock"),
                    (Tab::MagForce, "MagForce"),
                    (Tab::Power, "Power / GPU"),
                    (Tab::Profiles, "Profiles"),
                    (Tab::Diagnostics, "Diagnostics"),
                    (Tab::Recovery, "Recovery"),
                ] {
                    if ui
                        .selectable_label(self.tab == tab, RichText::new(label).size(14.0))
                        .clicked()
                    {
                        self.tab = tab;
                    }
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
                    ui.label(RichText::new("dry-run").color(Color32::from_rgb(120, 190, 255)));
                }
                ui.add_space(12.0);
                ui.separator();
                ui.label(
                    RichText::new("SNAPSHOT")
                        .small()
                        .color(Color32::from_gray(130)),
                );
                let latest = safety::latest_path();
                ui.label(
                    RichText::new(if latest.exists() {
                        latest.display().to_string()
                    } else {
                        "none — press Snapshot".to_string()
                    })
                    .small()
                    .color(Color32::from_gray(150)),
                );
                ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                    ui.label(
                        RichText::new(env!("CARGO_PKG_VERSION"))
                            .small()
                            .color(Color32::from_gray(100)),
                    );
                    ui.label(
                        RichText::new("PredatorSense Linux / PH717")
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
                    ui.label(RichText::new(&self.status).color(Color32::from_rgb(200, 220, 255)));
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

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.drain_monitor();
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
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
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
                        eprintln!("predator-sense-gui: screenshot failed: {e}");
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
        ui.ctx().request_repaint_after(Duration::from_millis(500));
    }
}

fn main() -> eframe::Result<()> {
    let mut self_test = false;
    let mut initial_tab = Tab::Dashboard;
    let mut screenshot_path = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--self-test" => self_test = true,
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
    let inner_size = if screenshot_path.is_some() {
        [1500.0, 1000.0]
    } else {
        [1320.0, 900.0]
    };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size(inner_size)
            .with_min_inner_size([980.0, 680.0])
            .with_title("PredatorSense Linux"),
        ..Default::default()
    };
    eframe::run_native(
        "PredatorSense Linux",
        options,
        Box::new(move |cc| {
            Ok(Box::new(App::new(
                cc,
                self_test,
                initial_tab,
                screenshot_path,
            )))
        }),
    )
}
