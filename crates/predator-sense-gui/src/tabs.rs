use std::path::Path;

use eframe::egui::{self, Color32, ComboBox, DragValue, RichText, Slider};
use predator_core::keymap;
use predator_core::lighting::LightingMode;
use predator_core::oc;
use predator_core::profile::{FanKind, LightingProfile};
use predator_core::protocol::{Rgb, EFFECTS};
use predator_core::safety;

use crate::keyboard::{self, metric, sparkline};
use crate::{App, CYAN, TEXT};

fn opt(v: Option<f32>, suffix: &str, digits: usize) -> String {
    v.map(|x| format!("{x:.digits$}{suffix}"))
        .unwrap_or_else(|| "—".to_string())
}

fn parse_hex(s: &str) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    for part in s.split(|c: char| c == ',' || c.is_whitespace()) {
        let p = part
            .trim()
            .trim_start_matches("0x")
            .trim_start_matches("0X");
        if p.is_empty() {
            continue;
        }
        out.push(u8::from_str_radix(p, 16).map_err(|_| format!("invalid byte {p:?}"))?);
    }
    Ok(out)
}

fn color_editor(ui: &mut egui::Ui, color: &mut Rgb) {
    let mut arr = color.to_array();
    ui.color_edit_button_srgb(&mut arr);
    *color = Rgb::from_array(arr);
    ui.label(RichText::new(color.hex()).monospace().small());
    for c in [
        Rgb::new(255, 0, 0),
        Rgb::new(0, 255, 0),
        Rgb::new(0, 0, 255),
        Rgb::new(255, 255, 255),
        Rgb::new(0, 174, 199),
        Rgb::new(255, 140, 0),
        Rgb::new(160, 0, 255),
    ] {
        let mut b = egui::Button::new(" ").min_size(egui::vec2(18.0, 18.0));
        b = b.fill(keyboard::rgb_color(c));
        if ui.add(b).clicked() {
            *color = c;
        }
    }
}

impl App {
    pub(crate) fn dashboard(&mut self, ui: &mut egui::Ui) {
        ui.heading("Home");
        ui.label(
            RichText::new("Live sensors from hwmon, cpufreq, RAPL and nvidia-smi.")
                .color(Color32::from_gray(150)),
        );
        ui.add_space(6.0);

        let cpu_temp = self.sample.cpu_temp.unwrap_or(0.0);
        let gpu_temp = self.sample.gpu_temp.unwrap_or(0.0);
        let cpu_clock = self.sample.cpu_freq_mhz.unwrap_or(0.0);
        let gpu_clock = self.sample.gpu_clock_mhz.unwrap_or(0.0);
        let gpu_load = self.sample.gpu_load.unwrap_or(0.0);
        ui.columns(2, |cols| {
            keyboard::gauge(
                &mut cols[0],
                "CPU TEMPERATURE",
                cpu_temp,
                100.0,
                "°C",
                &format!("{cpu_clock:.0} MHz"),
                CYAN,
            );
            keyboard::gauge(
                &mut cols[1],
                "GPU TEMPERATURE",
                gpu_temp,
                100.0,
                "°C",
                &format!("{gpu_clock:.0} MHz · {gpu_load:.0}%"),
                Color32::from_rgb(90, 200, 255),
            );
        });
        ui.add_space(6.0);

        let s = &self.sample;
        let metrics: [(&str, String, Color32); 10] = [
            (
                "CPU temperature",
                opt(s.cpu_temp, " °C", 1),
                Color32::from_rgb(255, 130, 110),
            ),
            (
                "GPU temperature",
                opt(s.gpu_temp, " °C", 1),
                Color32::from_rgb(110, 200, 255),
            ),
            (
                "CPU load",
                opt(s.cpu_load, " %", 1),
                Color32::from_rgb(255, 210, 100),
            ),
            (
                "GPU load",
                opt(s.gpu_load, " %", 1),
                Color32::from_rgb(150, 235, 150),
            ),
            (
                "CPU clock",
                opt(s.cpu_freq_mhz, " MHz", 0),
                Color32::from_rgb(220, 220, 230),
            ),
            (
                "GPU clock",
                opt(s.gpu_clock_mhz, " MHz", 0),
                Color32::from_rgb(220, 220, 230),
            ),
            (
                "CPU package",
                opt(s.cpu_power_w, " W", 1),
                Color32::from_rgb(255, 180, 120),
            ),
            (
                "GPU power",
                opt(s.gpu_power_w, " W", 1),
                Color32::from_rgb(255, 180, 120),
            ),
            (
                "Memory",
                format!(
                    "{} / {} GB",
                    opt(s.mem_used_gb, "", 1),
                    opt(s.mem_total_gb, "", 1)
                ),
                Color32::from_rgb(200, 200, 210),
            ),
            (
                "Battery",
                opt(s.battery_percent, " %", 0),
                Color32::from_rgb(180, 220, 255),
            ),
        ];
        egui::Grid::new("home_metrics")
            .num_columns(4)
            .spacing([10.0, 8.0])
            .show(ui, |ui| {
                for (i, (label, value, color)) in metrics.iter().enumerate() {
                    metric(ui, label, value.clone(), *color);
                    if (i + 1) % 4 == 0 {
                        ui.end_row();
                    }
                }
            });

        ui.add_space(10.0);
        ui.label(
            RichText::new("Temperature / load history")
                .small()
                .color(Color32::from_gray(150)),
        );
        sparkline(ui, &self.history);
        ui.add_space(10.0);

        ui.columns(2, |cols| {
            cols[0].heading("Fans");
            if s.fans.is_empty() {
                cols[0].label(
                    RichText::new("No fan tachometer exposed by hwmon on this kernel.")
                        .color(Color32::from_gray(160)),
                );
            } else {
                egui::Grid::new("fans")
                    .num_columns(2)
                    .show(&mut cols[0], |ui| {
                        for f in &s.fans {
                            ui.label(&f.name);
                            ui.label(format!("{} rpm", f.rpm));
                            ui.end_row();
                        }
                    });
            }
            cols[0].add_space(8.0);
            if let Some(wmi) = &self.controller.wmi {
                let _ = wmi;
                cols[0].label(
                    RichText::new("Fan duty is controlled through the EC (Fan control tab).")
                        .small()
                        .color(Color32::from_gray(150)),
                );
            }

            cols[1].heading("Devices");
            egui::Grid::new("devices")
                .num_columns(2)
                .show(&mut cols[1], |ui| {
                    ui.label("Model");
                    ui.label(&self.controller.status.model);
                    ui.end_row();
                    ui.label("Board");
                    ui.label(&self.controller.status.board);
                    ui.end_row();
                    ui.label("BIOS");
                    ui.label(&self.controller.status.bios);
                    ui.end_row();
                    ui.label("WMI");
                    ui.label(if self.controller.status.wmi {
                        "ok"
                    } else {
                        "missing"
                    });
                    ui.end_row();
                    ui.label("HID");
                    ui.label(if self.controller.status.hid {
                        "ok"
                    } else {
                        "missing"
                    });
                    ui.end_row();
                    if let Some(p) = &self.controller.status.hid_path {
                        ui.label("hidraw");
                        ui.label(p);
                        ui.end_row();
                    }
                    if let Some(g) = &s.gpu_name {
                        ui.label("GPU");
                        ui.label(g);
                        ui.end_row();
                    }
                    if let Some(t) = &s.battery_status {
                        ui.label("Battery");
                        ui.label(t);
                        ui.end_row();
                    }
                });
        });
    }

    pub(crate) fn lighting_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading("Lighting");
        if !self.controller.status.hid {
            ui.colored_label(
                Color32::from_rgb(255, 120, 120),
                self.controller
                    .status
                    .hid_error
                    .as_deref()
                    .unwrap_or("keyboard HID device unavailable"),
            );
            ui.label(
                "Install packaging/99-predator-sense.rules and add your user to the input group.",
            );
        }

        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            ui.label("Mode");
            ui.radio_value(&mut self.state.mode, LightingMode::Static, "Static per-key");
            ui.radio_value(&mut self.state.mode, LightingMode::Effect, "Effect");
            ui.radio_value(&mut self.state.mode, LightingMode::Off, "Off");
        });

        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            ui.label("Brightness");
            ui.add(Slider::new(&mut self.state.brightness, 0..=100).suffix("%"));
        });

        ui.horizontal_wrapped(|ui| {
            ui.label("Colour");
            color_editor(ui, &mut self.state.color);
            // keep the base colour if the per-key map is not explicitly edited
            if self.state.mode == LightingMode::Static {
                self.paint = self.state.color.to_array();
            }
        });

        if self.state.mode == LightingMode::Effect {
            ui.horizontal_wrapped(|ui| {
                ui.label("Effect");
                let current = predator_core::protocol::effect_name(self.state.effect);
                ComboBox::from_id_salt("effect")
                    .selected_text(current)
                    .show_ui(ui, |ui| {
                        for (id, name) in EFFECTS {
                            ui.selectable_value(&mut self.state.effect, *id, *name);
                        }
                    });
                ui.label("Speed");
                ui.add(Slider::new(&mut self.state.speed, 0..=10));
                ui.label("Direction");
                ComboBox::from_id_salt("direction")
                    .selected_text(if self.state.direction == 1 {
                        "Forward"
                    } else {
                        "Reverse"
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.state.direction, 1, "Forward");
                        ui.selectable_value(&mut self.state.direction, 0, "Reverse");
                    });
            });
        }

        ui.separator();
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("Per-key editor").strong());
            ui.label(
                RichText::new(format!("{} keys selected", self.selected.len()))
                    .small()
                    .color(Color32::from_gray(160)),
            );
            if ui.button("Select all").clicked() {
                self.selected = keymap::keys().iter().map(|k| k.tag).collect();
            }
            if ui.button("Clear").clicked() {
                self.selected.clear();
            }
            for preset in keymap::PRESETS {
                if ui.button(*preset).clicked() {
                    self.selected.extend(keymap::preset(preset));
                }
            }
            if ui.button("Paint selection").clicked() {
                let c = Rgb::from_array(self.paint);
                for t in &self.selected {
                    self.state.keys[*t as usize] = c;
                }
            }
        });
        ui.label(
            RichText::new("Click or drag on the keyboard to select. The colour picker above paints the selection.")
                .small()
                .color(Color32::from_gray(150)),
        );

        if let Some(tag) = keyboard::draw_keyboard(
            ui,
            &self.state.keys,
            &self.selected,
            self.controller.status.hid,
            !self.screenshot_mode,
        ) {
            self.selected.insert(tag);
        }

        ui.horizontal_wrapped(|ui| {
            if ui
                .button(RichText::new("Apply keyboard").strong())
                .clicked()
            {
                let state = self.state.clone();
                let r = self.controller.apply_lighting(state);
                self.report(r);
            }
            if ui.button("Save to profile").clicked() {
                self.profile.lighting = LightingProfile::from_state(&self.state);
                self.profile.name = self.profile_name.clone();
                match self.store.save(&self.profile) {
                    Ok(p) => self.status = format!("saved {}", p.display()),
                    Err(e) => self.status = format!("error: {e}"),
                }
            }
            if ui.button("Reload profile").clicked() {
                match self.store.load(&self.profile_name) {
                    Ok(p) => {
                        self.profile = p;
                        self.state = self.profile.lighting.state();
                        self.paint = self.state.color.to_array();
                        self.status = format!("loaded {}", self.profile_name);
                    }
                    Err(e) => self.status = format!("error: {e}"),
                }
            }
        });

        ui.separator();
        ui.heading("Chassis lights");
        ui.horizontal_wrapped(|ui| {
            ui.checkbox(&mut self.profile.ring.enabled, "Touchpad ring");
            ui.add(Slider::new(&mut self.profile.ring.brightness, 0..=100).text("brightness"));
            ui.label("colour");
            color_editor(ui, &mut self.profile.ring.color);
            if ui.button("Apply ring").clicked() {
                let (on, bright, color) = (
                    self.profile.ring.enabled,
                    self.profile.ring.brightness,
                    self.profile.ring.color,
                );
                let r = self.controller.apply_ring(on, bright, color);
                self.report(r);
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.checkbox(&mut self.profile.heatpipe, "Heat-pipe light");
            if ui.button("Apply heat-pipe").clicked() {
                let on = self.profile.heatpipe;
                let r = self.controller.apply_heatpipe(on);
                self.report(r);
            }
        });
        ui.label(
            RichText::new("On this 2019 firmware the ring is on/off only; colour writes are accepted but ignored.")
                .small()
                .color(Color32::from_gray(150)),
        );
    }

    fn fan_editor(&mut self, ui: &mut egui::Ui, kind: FanKind) {
        let mut s = self.profile.fan.get(kind).clone();
        let mut apply = false;
        egui::Frame::group(ui.style())
            .inner_margin(egui::Margin::same(10))
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(RichText::new(kind.label()).strong());
                    ui.checkbox(&mut s.auto, "Automatic");
                });
                ui.add_enabled(
                    !s.auto,
                    Slider::new(&mut s.percent, 20..=100).text("duty %"),
                );
                let button =
                    egui::Button::new("Apply").min_size(egui::vec2(ui.available_width(), 24.0));
                if ui.add(button).clicked() {
                    apply = true;
                }
            });
        self.profile.fan.get_mut(kind).clone_from(&s);
        if apply {
            let r = self.controller.apply_fan(kind, s.auto, s.percent);
            self.report(r);
        }
    }

    pub(crate) fn fans_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading("Fan control");
        ui.label(
            RichText::new("Manual duty is written to the EC through WMI methods 14/16, the same payloads PredatorSense uses.")
                .color(Color32::from_gray(150)),
        );
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            if ui.button("All automatic").clicked() {
                let mut err = None;
                for kind in [FanKind::Cpu, FanKind::Gpu] {
                    if let Err(e) = self.controller.apply_fan(kind, true, 0) {
                        err = Some(format!("{e}"));
                    }
                }
                match err {
                    Some(e) => self.status = format!("error: {e}"),
                    None => self.status = "fans automatic".to_string(),
                }
            }
            if ui.button("Refresh EC readback").clicked() {
                if let Some(wmi) = self.controller.wmi.clone() {
                    let mut lines = Vec::new();
                    for (label, sel) in [
                        ("cpu", 1u64),
                        ("sys1", 2),
                        ("sys2", 3),
                        ("gpu1", 4),
                        ("gpu2", 5),
                    ] {
                        let beh = wmi.get_fan_behavior(sel).unwrap_or(0);
                        let spd = wmi.get_fan_speed(sel).unwrap_or(0);
                        lines.push(format!(
                            "{label:5} behavior=0x{beh:016X} speed=0x{spd:016X} duty={}%",
                            (spd >> 8) & 0xFF
                        ));
                    }
                    self.fan_status = lines.join("\n");
                } else {
                    self.fan_status = "WMI unavailable".to_string();
                }
            }
        });
        ui.horizontal_wrapped(|ui| {
            if ui.button("Quiet (35%)").clicked() {
                self.snapshot_now();
                for kind in [FanKind::Cpu, FanKind::Gpu] {
                    let s = self.profile.fan.get_mut(kind);
                    s.auto = false;
                    s.percent = 35;
                    let r = self.controller.apply_fan(kind, false, 35);
                    self.report(r);
                }
            }
            if ui.button("Performance (90%)").clicked() {
                self.snapshot_now();
                for kind in [FanKind::Cpu, FanKind::Gpu] {
                    let s = self.profile.fan.get_mut(kind);
                    s.auto = false;
                    s.percent = 90;
                    let r = self.controller.apply_fan(kind, false, 90);
                    self.report(r);
                }
            }
        });
        ui.columns(2, |cols| {
            self.fan_editor(&mut cols[0], FanKind::Cpu);
            self.fan_editor(&mut cols[1], FanKind::Gpu);
        });
        ui.collapsing("Secondary fan groups (models with four fans)", |ui| {
            ui.columns(3, |cols| {
                self.fan_editor(&mut cols[0], FanKind::Cpu2);
                self.fan_editor(&mut cols[1], FanKind::Gpu2);
                self.fan_editor(&mut cols[2], FanKind::System);
            });
        });
        if !self.fan_status.is_empty() {
            ui.separator();
            ui.label(RichText::new("EC readback").strong());
            ui.label(RichText::new(&self.fan_status).monospace().small());
        }
    }

    pub(crate) fn overclock_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading("Overclocking");
        ui.label(
            RichText::new(
                "EC overclock writes change firmware limits. Use AC power and a recovery snapshot; never raise levels blindly.",
            )
            .color(Color32::from_rgb(255, 180, 90)),
        );
        ui.add_space(4.0);
        ui.checkbox(
            &mut self.allow_oc,
            "Enable EC overclock writes (AC + at least 40% battery required)",
        );
        self.controller.policy.allow_oc = self.allow_oc;
        ui.add_space(6.0);
        ui.vertical_centered(|ui| {
            ui.label(
                RichText::new("GPU")
                    .size(13.0)
                    .color(Color32::from_gray(185)),
            );
        });
        let selected_mode = self.gpu_mode();
        let max_clock = self
            .nvidia_power
            .as_ref()
            .and_then(|n| n.max_clock_mhz)
            .unwrap_or(2100.0);
        ui.horizontal_wrapped(|ui| {
            let total = 3.0 * 118.0 + 2.0 * 10.0;
            ui.add_space(((ui.available_width() - total) / 2.0).max(0.0));
            for (idx, name, spec) in [
                (0usize, "Normal", "stock limits"),
                (1, "Fast", "100 W / 1.5 GHz"),
                (2, "Extreme", "max power / clock"),
            ] {
                ui.vertical(|ui| {
                    let is = selected_mode == idx;
                    let fill = if is {
                        Color32::from_rgb(0, 74, 86)
                    } else {
                        Color32::from_rgb(13, 19, 25)
                    };
                    let stroke = egui::Stroke::new(
                        if is { 2.0 } else { 1.0 },
                        if is {
                            CYAN
                        } else {
                            Color32::from_rgb(26, 50, 60)
                        },
                    );
                    let button =
                        egui::Button::new(RichText::new(name).size(15.0).strong().color(if is {
                            CYAN
                        } else {
                            TEXT
                        }))
                        .fill(fill)
                        .stroke(stroke)
                        .corner_radius(6.0)
                        .min_size(egui::vec2(118.0, 46.0));
                    if ui.add(button).clicked() {
                        self.apply_gpu_mode(idx);
                    }
                    ui.label(RichText::new(spec).size(11.0).color(if is {
                        CYAN
                    } else {
                        Color32::from_gray(150)
                    }));
                });
                ui.add_space(10.0);
            }
        });
        ui.add_space(4.0);
        let clock = self.sample.gpu_clock_mhz.unwrap_or(0.0);
        let load = self.sample.gpu_load.unwrap_or(0.0);
        ui.vertical_centered(|ui| {
            keyboard::gauge(
                ui,
                "GPU CORE CLOCK",
                clock,
                max_clock.max(1.0),
                "MHz",
                &format!("Loading {load:.0}%"),
                Color32::from_rgb(90, 200, 255),
            );
        });
        ui.add_space(4.0);
        ui.separator();

        ui.label(RichText::new("Turbo (keyboard slide)").strong());
        ui.horizontal_wrapped(|ui| {
            let (slide, color) = match self.sample.slide_keyboard {
                Some(true) => ("engaged", Color32::from_rgb(255, 130, 50)),
                Some(false) => ("closed", Color32::from_gray(150)),
                None => ("unknown", Color32::from_gray(110)),
            };
            ui.label(RichText::new(format!("Keyboard slide: {slide}")).color(color));
            ui.checkbox(&mut self.auto_turbo, "Auto Turbo on slide");
            if ui
                .add_enabled(self.allow_oc, egui::Button::new("Apply Turbo now"))
                .clicked()
            {
                self.engage_turbo();
            }
            if ui
                .add_enabled(self.turbo_active, egui::Button::new("Restore Turbo"))
                .clicked()
            {
                self.release_turbo();
            }
        });
        ui.label(
            RichText::new("Turbo: fans 100%, heat-pipe/fan/ring lights on, orange keyboard wave, performance CPU/GPU limits.")
                .small()
                .color(Color32::from_gray(150)),
        );
        ui.separator();

        ui.label(
            RichText::new("EC legacy levels (accepted by firmware; the Windows XTU profile path is not available)")
                .strong(),
        );
        ui.horizontal_wrapped(|ui| {
            ui.label("CPU");
            for (level, name) in [(0u8, "Normal"), (1, "Fast"), (2, "Turbo")] {
                ui.radio_value(&mut self.profile.oc.cpu_level, level, name);
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.label("GPU");
            for (level, name) in [(0u8, "Normal"), (1, "Fast"), (2, "Turbo")] {
                ui.radio_value(&mut self.profile.oc.gpu_level, level, name);
            }
        });
        if ui
            .button(RichText::new("Apply firmware OC").strong())
            .clicked()
        {
            let p = self.profile.oc.clone();
            let r = self.controller.apply_oc(&p);
            self.report(r);
        }

        ui.add_space(8.0);
        ui.collapsing("Max-Q / GPU mode / POST sound", |ui| {
            let mut maxq = self.profile.oc.maxq_mode.is_some();
            if ui.checkbox(&mut maxq, "Max-Q level").changed() {
                self.profile.oc.maxq_mode = if maxq { Some(1) } else { None };
                self.profile.oc.maxq_level = if maxq { Some(0) } else { None };
            }
            if maxq {
                let mode = self.profile.oc.maxq_mode.get_or_insert(1);
                let level = self.profile.oc.maxq_level.get_or_insert(0);
                ComboBox::from_id_salt("maxq_mode")
                    .selected_text(format!("mode {mode}"))
                    .show_ui(ui, |ui| {
                        for m in 1..=2 {
                            ui.selectable_value(mode, m, format!("mode {m}"));
                        }
                    });
                ComboBox::from_id_salt("maxq_level")
                    .selected_text(format!("level {level}"))
                    .show_ui(ui, |ui| {
                        for l in 0..=2 {
                            ui.selectable_value(level, l, format!("level {l}"));
                        }
                    });
            }
            let mut gpu_mode = self.profile.oc.gpu_mode;
            ComboBox::from_id_salt("gpu_mode")
                .selected_text(match gpu_mode {
                    None => "GPU mode untouched".to_string(),
                    Some(v) => format!("GPU mode {v}"),
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut gpu_mode, None, "untouched");
                    for v in 1..=3u8 {
                        ui.selectable_value(&mut gpu_mode, Some(v), format!("mode {v}"));
                    }
                });
            self.profile.oc.gpu_mode = gpu_mode;
            let mut sound = self.profile.oc.post_sound.unwrap_or(true);
            if ui.checkbox(&mut sound, "POST animation sound").changed() {
                self.profile.oc.post_sound = Some(sound);
            }
            if ui.button("Apply Max-Q / mode / sound").clicked() {
                let p = self.profile.oc.clone();
                let r = self.controller.apply_oc(&p);
                self.report(r);
            }
        });

        ui.add_space(8.0);
        ui.separator();
        ui.label(RichText::new("Linux CPU controls (reversible)").strong());
        let st = oc::cpu_status();
        ui.label(
            RichText::new(format!(
                "driver {:?}   max {} MHz   current {} MHz   msr {}",
                st.driver,
                st.max_freq_khz.unwrap_or(0) / 1000,
                st.cur_freq_khz.unwrap_or(0) / 1000,
                if st.msr_available {
                    "available"
                } else {
                    "not exposed"
                }
            ))
            .monospace()
            .small(),
        );
        ui.horizontal_wrapped(|ui| {
            ui.label("Governor");
            ComboBox::from_id_salt("governor")
                .selected_text(&self.linux_governor)
                .show_ui(ui, |ui| {
                    for g in &st.available_governors {
                        ui.selectable_value(&mut self.linux_governor, g.clone(), g);
                    }
                });
            ui.label("EPP");
            ComboBox::from_id_salt("epp")
                .selected_text(&self.linux_epp)
                .show_ui(ui, |ui| {
                    for e in &st.available_epp {
                        ui.selectable_value(&mut self.linux_epp, e.clone(), e);
                    }
                });
        });
        ui.checkbox(
            &mut self.linux_no_turbo,
            "Disable turbo (intel_pstate no_turbo)",
        );
        let mut max_khz = self.linux_max_freq_khz;
        ui.horizontal_wrapped(|ui| {
            ui.label("Max frequency");
            ui.add(
                DragValue::new(&mut max_khz)
                    .speed(100_000)
                    .range(400_000..=6_000_000),
            );
            ui.label("kHz");
        });
        self.linux_max_freq_khz = max_khz;
        if ui.button("Apply Linux CPU settings").clicked() {
            if !self.controller.policy.allow_writes {
                self.status = "writes are disabled".to_string();
            } else {
                self.snapshot_now();
                let mut err: Option<String> = None;
                let governor = self.linux_governor.clone();
                if let Err(e) = self.run_privileged(&["oc", "linux", "governor", &governor]) {
                    err = Some(e);
                }
                if err.is_none() {
                    let epp = self.linux_epp.clone();
                    if let Err(e) = self.run_privileged(&["oc", "linux", "epp", &epp]) {
                        err = Some(e);
                    }
                }
                if err.is_none() {
                    let no_turbo = if self.linux_no_turbo { "on" } else { "off" };
                    if let Err(e) = self.run_privileged(&["oc", "linux", "no-turbo", no_turbo]) {
                        err = Some(e);
                    }
                }
                if err.is_none() && self.linux_max_freq_khz > 0 {
                    let khz = self.linux_max_freq_khz.to_string();
                    if let Err(e) = self.run_privileged(&["oc", "linux", "max-freq", &khz]) {
                        err = Some(e);
                    }
                }
                match err {
                    Some(e) => self.status = format!("error: {e}"),
                    None => self.status = "Linux CPU settings applied (administrator)".to_string(),
                }
            }
        }
    }

    pub(crate) fn profiles_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading("Profiles");
        ui.horizontal_wrapped(|ui| {
            ui.label("Name");
            ui.text_edit_singleline(&mut self.profile_name);
            if ui.button("Save current").clicked() {
                self.profile.lighting = LightingProfile::from_state(&self.state);
                self.profile.name = self.profile_name.clone();
                match self.store.save(&self.profile) {
                    Ok(p) => self.status = format!("saved {}", p.display()),
                    Err(e) => self.status = format!("error: {e}"),
                }
            }
        });
        ui.add_space(6.0);

        let names = self.store.list().unwrap_or_default();
        if names.is_empty() {
            ui.label(RichText::new("No profiles yet.").color(Color32::from_gray(160)));
        }
        egui::ScrollArea::horizontal()
            .auto_shrink([false, false])
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded)
            .show(ui, |ui| {
                egui::Grid::new("profiles")
                    .num_columns(4)
                    .striped(true)
                    .show(ui, |ui| {
                        ui.label(RichText::new("Profile").strong());
                        ui.label(RichText::new("Load").strong());
                        ui.label(RichText::new("Apply").strong());
                        ui.label(RichText::new("Delete").strong());
                        ui.end_row();
                        for name in names {
                            ui.label(&name);
                            if ui.button("Load").clicked() {
                                match self.store.load(&name) {
                                    Ok(p) => {
                                        self.profile = p;
                                        self.state = self.profile.lighting.state();
                                        self.paint = self.state.color.to_array();
                                        self.profile_name = name.clone();
                                        self.sync_profile_fields();
                                        self.status = format!("loaded {name}");
                                    }
                                    Err(e) => self.status = format!("error: {e}"),
                                }
                            }
                            if ui.button("Apply").clicked() {
                                match self.store.load(&name) {
                                    Ok(p) => {
                                        self.profile = p.clone();
                                        self.state = self.profile.lighting.state();
                                        self.profile_name = name.clone();
                                        self.sync_profile_fields();
                                        let r = self.controller.apply_profile(&p);
                                        self.report(r);
                                    }
                                    Err(e) => self.status = format!("error: {e}"),
                                }
                            }
                            if ui.button("Delete").clicked() {
                                match self.store.delete(&name) {
                                    Ok(()) => self.status = format!("deleted {name}"),
                                    Err(e) => self.status = format!("error: {e}"),
                                }
                            }
                            ui.end_row();
                        }
                    });
            });

        ui.add_space(8.0);
        ui.separator();
        ui.heading("Backup / restore profiles");
        ui.horizontal_wrapped(|ui| {
            ui.label("Export");
            ui.text_edit_singleline(&mut self.export_path);
            if ui.button("Export").clicked() {
                match self.store.export(Path::new(&self.export_path)) {
                    Ok(n) => self.status = format!("exported {n} profiles"),
                    Err(e) => self.status = format!("error: {e}"),
                }
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.label("Import");
            ui.text_edit_singleline(&mut self.import_path);
            if ui.button("Import").clicked() {
                match self.store.import(Path::new(&self.import_path)) {
                    Ok(n) => self.status = format!("imported {n} profiles"),
                    Err(e) => self.status = format!("error: {e}"),
                }
            }
        });

        ui.add_space(8.0);
        ui.collapsing("Current profile TOML", |ui| {
            let p = self.current_profile();
            match toml::to_string_pretty(&p) {
                Ok(text) => {
                    egui::ScrollArea::both()
                        .max_height(320.0)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.add(
                                egui::Label::new(RichText::new(text).monospace().small()).extend(),
                            );
                        });
                }
                Err(e) => {
                    ui.label(format!("error: {e}"));
                }
            }
        });
    }

    pub(crate) fn diagnostics_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading("Diagnostics");
        egui::Grid::new("diag").num_columns(2).show(ui, |ui| {
            for (k, v) in [
                ("model", self.controller.status.model.clone()),
                ("board", self.controller.status.board.clone()),
                ("bios", self.controller.status.bios.clone()),
                ("kernel", self.controller.status.kernel.clone()),
                ("wmi", self.controller.status.wmi_path.clone()),
                (
                    "hid",
                    self.controller
                        .status
                        .hid_path
                        .clone()
                        .unwrap_or_else(|| "unavailable".into()),
                ),
                (
                    "hid phys",
                    self.controller.status.hid_phys.clone().unwrap_or_default(),
                ),
                (
                    "hid 0x88",
                    self.controller
                        .status
                        .hid_health
                        .clone()
                        .unwrap_or_default(),
                ),
                (
                    "hid 0xB0",
                    self.controller.status.hid_b0.clone().unwrap_or_default(),
                ),
            ] {
                ui.label(k);
                ui.label(RichText::new(v).monospace().small());
                ui.end_row();
            }
        });

        ui.separator();
        ui.heading("Raw WMI");
        ui.horizontal_wrapped(|ui| {
            ui.label("method");
            ui.add(DragValue::new(&mut self.raw_method).range(0..=255));
            ui.label("payload");
            ui.text_edit_singleline(&mut self.raw_payload);
            if ui.button("Send").clicked() {
                self.raw_response = match self.controller.wmi.as_ref() {
                    None => "WMI unavailable".to_string(),
                    Some(wmi) => match parse_hex(&self.raw_payload) {
                        Err(e) => format!("payload: {e}"),
                        Ok(bytes) => match wmi.raw(self.raw_method, &bytes) {
                            Ok(r) => format!("response: {}", predator_core::wmi::hexs(&r)),
                            Err(e) => format!("error: {e}"),
                        },
                    },
                };
            }
        });
        if !self.raw_response.is_empty() {
            ui.label(RichText::new(&self.raw_response).monospace().small());
        }

        ui.separator();
        ui.heading("HID feature command");
        ui.horizontal_wrapped(|ui| {
            ui.label("opcode");
            ui.add(DragValue::new(&mut self.hid_opcode).range(0..=255));
            if ui.button("Get").clicked() {
                self.hid_response = match self.controller.hid.as_ref() {
                    None => "HID unavailable".to_string(),
                    Some(hid) => {
                        let mut cmd = [0u8; 8];
                        cmd[0] = self.hid_opcode;
                        match hid.raw_get(cmd) {
                            Ok(r) => format!("response: {}", predator_core::wmi::hexs(&r)),
                            Err(e) => format!("error: {e}"),
                        }
                    }
                };
            }
        });
        if !self.hid_response.is_empty() {
            ui.label(RichText::new(&self.hid_response).monospace().small());
        }

        ui.separator();
        ui.heading("Log");
        egui::ScrollArea::both()
            .max_height(220.0)
            .auto_shrink([false, false])
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded)
            .show(ui, |ui| {
                for line in self.controller.log.iter().rev() {
                    ui.add(egui::Label::new(RichText::new(line).monospace().small()).extend());
                }
            });
    }

    pub(crate) fn recovery_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading("Recovery");
        ui.label(
            RichText::new("Every write here is a reversible WMI mailbox call or HID command. No EC or SPI firmware is written.")
                .color(Color32::from_gray(160)),
        );
        ui.add_space(8.0);

        ui.horizontal_wrapped(|ui| {
            if ui.button(RichText::new("Snapshot now").strong()).clicked() {
                let p = self.current_profile();
                match self.controller.save_state(&p) {
                    Ok(path) => {
                        self.status = format!("snapshot: {}", path.display());
                    }
                    Err(e) => self.status = format!("error: {e}"),
                }
            }
            if ui.button("Restore latest (safe state)").clicked() {
                let r = self.controller.restore_latest(false);
                self.report(r);
            }
            if ui.button("Restore latest + profile").clicked() {
                let r = self.controller.restore_latest(true);
                self.report(r);
            }
        });
        ui.add_space(6.0);
        let panic = egui::Button::new(
            RichText::new("PANIC RESET — lights off, fans auto, OC normal").strong(),
        )
        .fill(Color32::from_rgb(120, 30, 30));
        if ui.add(panic).clicked() {
            let r = self.controller.panic();
            self.report(r);
        }

        ui.add_space(10.0);
        ui.separator();
        ui.heading("Backup locations");
        let latest = safety::latest_path();
        ui.label(
            RichText::new(format!("latest snapshot: {}", latest.display()))
                .monospace()
                .small(),
        );
        ui.label(
            RichText::new(format!("profiles:        {}", self.store.root().display()))
                .monospace()
                .small(),
        );
        let snaps = safety::list_snapshots().unwrap_or_default();
        ui.label(format!("{} historical snapshots", snaps.len()));

        ui.add_space(10.0);
        ui.separator();
        ui.heading("If something goes wrong");
        ui.label("1. Press PANIC RESET, or run: predator-sense panic");
        ui.label(
            "2. If the keyboard is unresponsive, use an external USB keyboard and power-cycle.",
        );
        ui.label("3. Hold the power button for 15 s (EC reset), then boot with module_blacklist=acer_gaming_probe if needed.");
        ui.label("4. For OC instability: enter the BIOS, restore defaults, then run: predator-sense oc cpu 0");
        ui.label("5. Remove the udev rule and DKMS module to return to a stock system.");
        if ui.button("Open recovery guide").clicked() {
            let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/RECOVERY.md");
            let _ = std::process::Command::new("xdg-open").arg(path).spawn();
        }
        ui.add_space(8.0);
        ui.label(
            RichText::new(
                "The full guide is docs/RECOVERY.md. Keep the latest snapshot when experimenting.",
            )
            .small()
            .color(Color32::from_gray(150)),
        );
    }
}

impl App {
    pub(crate) fn magforce_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading("MagForce");
        ui.label(
            RichText::new("The PH717 uses MagForce optical-mechanical keys. Actuation, switch mode, analog curve and game-key mapping are separate HID commands (0x30–0x35).")
                .color(Color32::from_gray(160)),
        );
        if !self.controller.status.hid {
            ui.colored_label(
                Color32::from_rgb(255, 120, 120),
                "keyboard HID device unavailable; MagForce controls are disabled",
            );
        }
        let enabled = self.controller.status.hid && !self.read_only && !self.dry_run;

        ui.add_space(6.0);
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.label(RichText::new("Actuation point").strong());
            ui.add_enabled(
                enabled,
                Slider::new(&mut self.mag_actuation, 0..=255).text("raw 0x32 value"),
            );
            if ui
                .add_enabled(enabled, egui::Button::new("Apply actuation"))
                .clicked()
            {
                match self.controller.hid.as_ref() {
                    None => self.status = "HID unavailable".into(),
                    Some(hid) => match hid.set_actuation_point(self.mag_actuation) {
                        Ok(()) => self.status = format!("actuation set to {}", self.mag_actuation),
                        Err(e) => self.status = format!("error: {e}"),
                    },
                }
            }
        });

        ui.add_space(4.0);
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.label(RichText::new("Switch / linear / joystick").strong());
            ui.horizontal_wrapped(|ui| {
                ui.label("switch");
                ComboBox::from_id_salt("mag_switch")
                    .selected_text(if self.mag_switch == 0 {
                        "Standard"
                    } else {
                        "Linear"
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.mag_switch, 0, "Standard");
                        ui.selectable_value(&mut self.mag_switch, 1, "Linear");
                    });
                ui.label("linear");
                ComboBox::from_id_salt("mag_linear")
                    .selected_text(if self.mag_linear == 0 {
                        "Typing"
                    } else {
                        "Gaming"
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.mag_linear, 0, "Typing");
                        ui.selectable_value(&mut self.mag_linear, 1, "Gaming");
                    });
                ui.checkbox(&mut self.mag_joystick, "joystick");
            });
            if ui
                .add_enabled(enabled, egui::Button::new("Apply key setting"))
                .clicked()
            {
                match self.controller.hid.as_ref() {
                    None => self.status = "HID unavailable".into(),
                    Some(hid) => match hid.set_key_setting(
                        self.mag_switch,
                        self.mag_linear,
                        self.mag_joystick as u8,
                    ) {
                        Ok(()) => self.status = "MagForce key setting applied".into(),
                        Err(e) => self.status = format!("error: {e}"),
                    },
                }
            }
        });

        ui.add_space(4.0);
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.label(RichText::new("Analog curve").strong());
            ui.add_enabled(enabled, Slider::new(&mut self.mag_analog, 0..=255));
            if ui
                .add_enabled(enabled, egui::Button::new("Apply analog curve"))
                .clicked()
            {
                match self.controller.hid.as_ref() {
                    None => self.status = "HID unavailable".into(),
                    Some(hid) => match hid.set_analog_curve(self.mag_analog) {
                        Ok(()) => self.status = "analog curve applied".into(),
                        Err(e) => self.status = format!("error: {e}"),
                    },
                }
            }
        });

        ui.add_space(4.0);
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.label(RichText::new("MagForce replace mode").strong());
            ui.horizontal_wrapped(|ui| {
                ui.checkbox(&mut self.mag_replace, "replace mode");
                ui.checkbox(&mut self.mag_replace_30s, "30-second backlight");
            });
            if ui
                .add_enabled(enabled, egui::Button::new("Apply replace mode"))
                .clicked()
            {
                match self.controller.hid.as_ref() {
                    None => self.status = "HID unavailable".into(),
                    Some(hid) => match hid
                        .set_magforce_replace_mode(self.mag_replace, self.mag_replace_30s)
                    {
                        Ok(()) => self.status = "replace mode applied".into(),
                        Err(e) => self.status = format!("error: {e}"),
                    },
                }
            }
        });

        ui.add_space(4.0);
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.label(RichText::new("Game keys (W / A / S / D)").strong());
            ui.horizontal_wrapped(|ui| {
                for (i, usage) in ["W", "A", "S", "D"].iter().enumerate() {
                    ui.label(*usage);
                    ui.add(DragValue::new(&mut self.mag_gamekeys[i]).range(0..=24));
                    ui.label(
                        RichText::new(format!("slot {}", self.mag_gamekeys[i]))
                            .small()
                            .color(Color32::from_gray(150)),
                    );
                }
            });
            if ui
                .add_enabled(enabled, egui::Button::new("Apply game keys"))
                .clicked()
            {
                match self.controller.hid.as_ref() {
                    None => self.status = "HID unavailable".into(),
                    Some(hid) => match hid.set_gamekeys(self.mag_gamekeys) {
                        Ok(()) => self.status = "game keys applied".into(),
                        Err(e) => self.status = format!("error: {e}"),
                    },
                }
            }
        });

        ui.add_space(6.0);
        if ui
            .add_enabled(
                self.controller.status.hid,
                egui::Button::new("Read MagForce getters"),
            )
            .clicked()
        {
            self.mag_status = match self.controller.hid.as_ref() {
                None => "HID unavailable".into(),
                Some(hid) => {
                    let mut lines = Vec::new();
                    for (label, r) in [
                        ("key setting", hid.get_key_setting()),
                        ("actuation", hid.get_actuation_point()),
                        ("analog curve", hid.get_analog_curve()),
                        ("30s backlight", hid.get_backlight_30s()),
                        ("game keys", hid.get_gamekeys()),
                    ] {
                        lines.push(match r {
                            Ok(v) => format!("{label:14} {}", predator_core::wmi::hexs(&v)),
                            Err(e) => format!("{label:14} error: {e}"),
                        });
                    }
                    lines.join("\n")
                }
            };
        }
        if !self.mag_status.is_empty() {
            ui.label(RichText::new(&self.mag_status).monospace().small());
        }
    }
}

impl App {
    pub(crate) fn sync_power_fields(&mut self) {
        if let Some(v) = self.profile.power.pl1_w {
            self.pl1_w = v as u32;
        }
        if let Some(v) = self.profile.power.pl2_w {
            self.pl2_w = v as u32;
        }
        if let Some(v) = self.profile.power.gpu_power_w {
            self.gpu_power_w = v as u32;
        }
        if let Some(v) = self.profile.power.gpu_lock_min_mhz {
            self.gpu_lock_min = v;
        }
        if let Some(v) = self.profile.power.gpu_lock_max_mhz {
            self.gpu_lock_max = v;
        }
    }

    pub(crate) fn power_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading("Power / GPU tuning");
        ui.label(
            RichText::new("Linux equivalents of the PredatorSense XTU/NVAPI layer. Acer ships the .xtu profiles encrypted, so these controls set the same class of limits directly through powercap and the NVIDIA driver. Writes need root and --allow-oc.")
                .color(Color32::from_gray(160)),
        );
        ui.add_space(6.0);
        ui.checkbox(&mut self.allow_oc, "Allow overclock / power-limit writes");
        self.controller.policy.allow_oc = self.allow_oc;
        let enabled = self.allow_oc && !self.read_only && !self.dry_run;

        ui.add_space(6.0);
        if ui.button("Refresh power/GPU status").clicked() {
            self.power_limits = oc::PowerLimits::read();
            self.nvidia_power = oc::nvidia_status();
        }
        ui.separator();
        ui.heading("CPU package power (RAPL)");
        let limits = &self.power_limits;
        ui.label(
            RichText::new(format!(
                "current PL1 {} W / PL2 {} W",
                limits
                    .pl1_uw
                    .map(|v| v as f64 / 1e6)
                    .map(|v| format!("{v:.1}"))
                    .unwrap_or_else(|| "—".into()),
                limits
                    .pl2_uw
                    .map(|v| v as f64 / 1e6)
                    .map(|v| format!("{v:.1}"))
                    .unwrap_or_else(|| "—".into()),
            ))
            .monospace()
            .small(),
        );
        ui.horizontal_wrapped(|ui| {
            ui.label("PL1");
            ui.add(DragValue::new(&mut self.pl1_w).range(5..=200).suffix(" W"));
            if ui
                .add_enabled(enabled, egui::Button::new("Set PL1"))
                .clicked()
            {
                self.snapshot_now();
                self.profile.power.pl1_w = Some(self.pl1_w as f32);
                let watts = self.pl1_w.to_string();
                match self.run_privileged(&["--allow-oc", "oc", "power", "pl1", &watts]) {
                    Ok(()) => {
                        self.status = format!("PL1 set to {} W", self.pl1_w);
                        self.power_limits = oc::PowerLimits::read();
                    }
                    Err(e) => self.status = format!("error: {e}"),
                }
            }
            ui.label("PL2");
            ui.add(DragValue::new(&mut self.pl2_w).range(5..=200).suffix(" W"));
            if ui
                .add_enabled(enabled, egui::Button::new("Set PL2"))
                .clicked()
            {
                self.snapshot_now();
                self.profile.power.pl2_w = Some(self.pl2_w as f32);
                let watts = self.pl2_w.to_string();
                match self.run_privileged(&["--allow-oc", "oc", "power", "pl2", &watts]) {
                    Ok(()) => {
                        self.status = format!("PL2 set to {} W", self.pl2_w);
                        self.power_limits = oc::PowerLimits::read();
                    }
                    Err(e) => self.status = format!("error: {e}"),
                }
            }
        });
        ui.label(
            RichText::new(
                "powercap files are root-writable; set the window only if your kernel accepts it.",
            )
            .small()
            .color(Color32::from_gray(150)),
        );

        ui.add_space(6.0);
        ui.separator();
        ui.heading("GPU power and clocks (NVIDIA)");
        if let Some(n) = &self.nvidia_power {
            let fmt = |v: Option<f32>, unit: &str| match v {
                Some(v) => format!("{v:.0}{unit}"),
                None => "—".to_string(),
            };
            let range = match (n.min_power_w, n.max_power_w) {
                (Some(lo), Some(hi)) => format!("range {lo:.0}–{hi:.0} W"),
                _ => "range unknown".to_string(),
            };
            ui.label(
                RichText::new(format!(
                    "power limit {} ({range})   max clock {}   current {}",
                    fmt(n.power_limit_w, " W"),
                    fmt(n.max_clock_mhz, " MHz"),
                    fmt(n.current_clock_mhz, " MHz"),
                ))
                .monospace()
                .small(),
            );
        } else {
            ui.label(RichText::new("nvidia-smi unavailable").color(Color32::from_gray(160)));
        }
        ui.horizontal_wrapped(|ui| {
            ui.label("power limit");
            ui.add(
                DragValue::new(&mut self.gpu_power_w)
                    .range(5..=500)
                    .suffix(" W"),
            );
            if ui
                .add_enabled(enabled, egui::Button::new("Set GPU power"))
                .clicked()
            {
                self.snapshot_now();
                self.profile.power.gpu_power_w = Some(self.gpu_power_w as f32);
                let watts = self.gpu_power_w.to_string();
                match self.run_privileged(&["--allow-oc", "oc", "nvidia", "power", &watts]) {
                    Ok(()) => {
                        self.status = format!("GPU power limit set to {} W", self.gpu_power_w);
                        self.nvidia_power = oc::nvidia_status();
                    }
                    Err(e) => self.status = format!("error: {e}"),
                }
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.label("lock clocks");
            ui.add(
                DragValue::new(&mut self.gpu_lock_min)
                    .range(0..=4000)
                    .suffix(" MHz"),
            );
            ui.label("to");
            ui.add(
                DragValue::new(&mut self.gpu_lock_max)
                    .range(0..=4000)
                    .suffix(" MHz"),
            );
            if ui.add_enabled(enabled, egui::Button::new("Lock")).clicked() {
                self.snapshot_now();
                self.profile.power.gpu_lock_min_mhz = Some(self.gpu_lock_min);
                self.profile.power.gpu_lock_max_mhz = Some(self.gpu_lock_max);
                let min = self.gpu_lock_min.to_string();
                let max = self.gpu_lock_max.to_string();
                match self.run_privileged(&["--allow-oc", "oc", "nvidia", "lock", &min, &max]) {
                    Ok(()) => {
                        self.status = "GPU clocks locked".into();
                        self.nvidia_power = oc::nvidia_status();
                    }
                    Err(e) => self.status = format!("error: {e}"),
                }
            }
            if ui
                .add_enabled(enabled, egui::Button::new("Reset clocks"))
                .clicked()
            {
                self.snapshot_now();
                self.profile.power.gpu_lock_min_mhz = None;
                self.profile.power.gpu_lock_max_mhz = None;
                match self.run_privileged(&["--allow-oc", "oc", "nvidia", "reset"]) {
                    Ok(()) => {
                        self.status = "GPU clocks reset".into();
                        self.nvidia_power = oc::nvidia_status();
                    }
                    Err(e) => self.status = format!("error: {e}"),
                }
            }
        });
        ui.label(
            RichText::new("NVML/nvidia-smi does not expose core/memory voltage offsets; those remain an XTU/Windows-only control.")
                .small()
                .color(Color32::from_gray(150)),
        );
    }
    fn gpu_mode(&self) -> usize {
        match self.profile.power.gpu_lock_max_mhz {
            Some(max) if max >= 2000.0 => 2,
            Some(max) if max >= 1000.0 => 1,
            _ => 0,
        }
    }

    fn apply_gpu_mode(&mut self, mode: usize) {
        if !self.allow_oc {
            self.status = "enable overclocking first".into();
            return;
        }
        self.snapshot_now();
        let max_w = self
            .nvidia_power
            .as_ref()
            .and_then(|n| n.max_power_w)
            .unwrap_or(115.0);
        let max_clock = self
            .nvidia_power
            .as_ref()
            .and_then(|n| n.max_clock_mhz)
            .unwrap_or(2100.0);
        let (watts, lo, hi) = match mode {
            0 => (max_w, None, None),
            1 => (
                100.0_f32.min(max_w),
                Some(300.0_f32),
                Some(1500.0_f32.min(max_clock)),
            ),
            _ => (max_w, Some(300.0_f32), Some(max_clock)),
        };
        let ws = format!("{watts:.0}");
        let mut result = self.run_privileged(&["--allow-oc", "oc", "nvidia", "power", &ws]);
        if result.is_ok() {
            result = match (lo, hi) {
                (Some(lo), Some(hi)) => {
                    let los = format!("{lo:.0}");
                    let his = format!("{hi:.0}");
                    self.run_privileged(&["--allow-oc", "oc", "nvidia", "lock", &los, &his])
                }
                _ => self.run_privileged(&["--allow-oc", "oc", "nvidia", "reset"]),
            };
        }
        match result {
            Ok(_) => {
                self.profile.power.gpu_power_w = Some(watts);
                self.profile.power.gpu_lock_min_mhz = lo;
                self.profile.power.gpu_lock_max_mhz = hi;
                self.gpu_power_w = watts as u32;
                if let Some(hi) = hi {
                    self.gpu_lock_max = hi;
                }
                self.nvidia_power = oc::nvidia_status();
                self.status = format!(
                    "GPU mode {}",
                    ["Normal", "Fast", "Extreme"].get(mode).unwrap_or(&"?")
                );
            }
            Err(e) => self.status = format!("GPU mode failed: {e}"),
        }
    }
}
