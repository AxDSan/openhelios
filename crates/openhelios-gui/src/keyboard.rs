use std::collections::{HashSet, VecDeque};

use eframe::egui::{
    self, Align2, Color32, FontId, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, Vec2,
};
use openhelios_core::keymap::{self, KEYS};
use openhelios_core::monitor::HistoryPoint;
use openhelios_core::protocol::Rgb;

pub fn rgb_color(c: Rgb) -> Color32 {
    Color32::from_rgb(c.r, c.g, c.b)
}

pub fn readable_text(c: Rgb) -> Color32 {
    let lum = 0.2126 * c.r as f32 + 0.7152 * c.g as f32 + 0.0722 * c.b as f32;
    if lum > 140.0 {
        Color32::from_rgb(10, 10, 10)
    } else {
        Color32::from_rgb(235, 235, 235)
    }
}

/// Draw the PH717 keyboard. Returns the tag under the pointer while the
/// primary button is down, so the caller can paint a selection by dragging.
pub fn draw_keyboard(
    ui: &mut egui::Ui,
    colors: &[Rgb; 128],
    selected: &HashSet<u8>,
    interactive: bool,
    show_hover: bool,
) -> Option<u8> {
    let max_x = KEYS.iter().fold(0.0f32, |m, k| m.max(k.x + k.w)) + 32.0;
    let max_y = KEYS.iter().fold(0.0f32, |m, k| m.max(k.y + k.h)) + 32.0;

    let avail = ui.available_size();
    let scale = ((avail.x - 48.0) / max_x)
        .min((avail.y.max(120.0) - 24.0) / max_y)
        .clamp(0.35, 1.0);
    let keyboard_w = max_x * scale;
    let canvas_w = ui.available_width().max(keyboard_w + 8.0);
    let size = Vec2::new(canvas_w, max_y * scale + 8.0);
    let (response, painter) = ui.allocate_painter(size, Sense::click_and_drag());
    let origin = Pos2::new(
        response.rect.min.x + (canvas_w - keyboard_w) / 2.0 + 16.0 * scale,
        response.rect.min.y + 16.0 * scale,
    );

    painter.rect_filled(response.rect, 6.0, Color32::from_rgb(16, 17, 20));

    let pointer = response
        .hover_pos()
        .or_else(|| ui.input(|i| i.pointer.interact_pos()));
    let down = interactive && response.hovered() && ui.input(|i| i.pointer.primary_down());

    let mut hit = None;
    for k in KEYS {
        let rect = Rect::from_min_size(
            origin + Vec2::new(k.x * scale, k.y * scale),
            Vec2::new(k.w * scale, k.h * scale),
        );
        if rect.width() < 2.0 || rect.height() < 2.0 {
            continue;
        }
        let rgb = colors[k.tag as usize];
        let is_selected = selected.contains(&k.tag);
        let fill = rgb_color(rgb);
        let stroke = if is_selected {
            Stroke::new(2.0, Color32::from_rgb(255, 190, 60))
        } else {
            Stroke::new(1.0, Color32::from_rgb(48, 50, 56))
        };
        painter.rect(rect.shrink(0.6), 3.0, fill, stroke, StrokeKind::Inside);

        let label = keymap::label(k);
        let font = (rect.height() * 0.34).clamp(6.5, 13.0);
        let text_color = if is_selected {
            Color32::WHITE
        } else {
            readable_text(rgb)
        };
        let clipped = ui.painter().with_clip_rect(rect.shrink(1.0));
        clipped.text(
            rect.center(),
            Align2::CENTER_CENTER,
            label,
            FontId::proportional(font),
            text_color,
        );

        if let Some(p) = pointer {
            if rect.contains(p) {
                hit = Some(k.tag);
                if show_hover {
                    let hover = Stroke::new(2.0, Color32::from_rgb(120, 190, 255));
                    painter.rect_stroke(rect.shrink(0.6), 3.0, hover, StrokeKind::Outside);
                    response.clone().on_hover_text(format!(
                        "{}  (tag {})\ncolour {}",
                        label,
                        k.tag,
                        rgb.hex()
                    ));
                }
            }
        }
    }

    if down || response.clicked() {
        hit
    } else {
        None
    }
}

pub fn metric(ui: &mut egui::Ui, label: &str, value: impl Into<String>, accent: Color32) {
    egui::Frame::group(ui.style())
        .fill(Color32::from_rgb(12, 18, 24))
        .stroke(Stroke::new(1.0, Color32::from_rgb(24, 56, 66)))
        .inner_margin(egui::Margin::same(10))
        .show(ui, |ui| {
            ui.set_min_width(142.0);
            ui.vertical(|ui| {
                ui.label(RichText::new(label).small().color(Color32::from_gray(160)));
                ui.label(RichText::new(value).size(23.0).strong().color(accent));
            });
        });
}

pub fn sparkline(ui: &mut egui::Ui, history: &VecDeque<HistoryPoint>) {
    let (resp, painter) =
        ui.allocate_painter(Vec2::new(ui.available_width(), 110.0), Sense::hover());
    let rect = resp.rect;
    painter.rect_filled(rect, 6.0, Color32::from_rgb(18, 19, 23));
    for i in 0..=4 {
        let y = rect.top() + rect.height() * i as f32 / 4.0;
        painter.line_segment(
            [Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)],
            Stroke::new(1.0, Color32::from_rgb(36, 38, 44)),
        );
    }
    if history.len() < 2 {
        painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            "collecting samples…",
            FontId::proportional(12.0),
            Color32::from_gray(120),
        );
        return;
    }
    let n = history.len();
    let x = |i: usize| rect.left() + rect.width() * i as f32 / (n - 1) as f32;
    let y = |v: f32| rect.bottom() - rect.height() * (v.clamp(0.0, 100.0) / 100.0);

    let line = |get: &dyn Fn(&HistoryPoint) -> f32, color: Color32| {
        let pts: Vec<Pos2> = history
            .iter()
            .enumerate()
            .map(|(i, h)| Pos2::new(x(i), y(get(h))))
            .collect();
        painter.add(egui::Shape::line(pts, Stroke::new(1.8, color)));
    };
    line(&|h| h.cpu_temp, Color32::from_rgb(255, 110, 90));
    line(&|h| h.gpu_temp, Color32::from_rgb(90, 200, 255));
    line(&|h| h.cpu_load, Color32::from_rgb(255, 200, 80));
    line(&|h| h.gpu_load, Color32::from_rgb(140, 240, 140));

    let legend = [
        ("CPU °C", Color32::from_rgb(255, 110, 90)),
        ("GPU °C", Color32::from_rgb(90, 200, 255)),
        ("CPU %", Color32::from_rgb(255, 200, 80)),
        ("GPU %", Color32::from_rgb(140, 240, 140)),
    ];
    let mut lx = rect.left() + 8.0;
    for (text, color) in legend {
        painter.circle_filled(Pos2::new(lx, rect.top() + 10.0), 3.5, color);
        painter.text(
            Pos2::new(lx + 8.0, rect.top() + 10.0),
            Align2::LEFT_CENTER,
            text,
            FontId::proportional(11.0),
            Color32::from_gray(190),
        );
        lx += 62.0;
    }
}

/// A OpenHelios circular gauge.
pub fn gauge(
    ui: &mut egui::Ui,
    label: &str,
    value: f32,
    max: f32,
    unit: &str,
    sub: &str,
    color: Color32,
) {
    let width = ui.available_width().clamp(180.0, 320.0);
    ui.vertical_centered(|ui| {
        let (response, painter) = ui.allocate_painter(Vec2::new(width, 210.0), Sense::hover());
        let rect = response.rect;
        let radius = ((rect.height() - 44.0) / 1.72).min(rect.width() * 0.38);
        let center = Pos2::new(rect.center().x, rect.top() + 27.0 + radius);
        let start = std::f32::consts::PI * 0.75;
        let end = std::f32::consts::PI * 2.25;

        let arc = |from: f32, to: f32, stroke: Stroke| {
            let mut points = Vec::with_capacity(64);
            for i in 0..=64 {
                let t = i as f32 / 64.0;
                let a = from + (to - from) * t;
                points.push(center + Vec2::new(a.cos(), a.sin()) * radius);
            }
            painter.add(egui::Shape::line(points, stroke));
        };

        arc(start, end, Stroke::new(11.0, Color32::from_rgb(19, 30, 38)));
        for i in 0..=10 {
            let a = start + (end - start) * (i as f32 / 10.0);
            let dir = Vec2::new(a.cos(), a.sin());
            painter.line_segment(
                [
                    center + dir * (radius * 0.80),
                    center + dir * (radius * 0.90),
                ],
                Stroke::new(1.0, Color32::from_rgb(45, 70, 82)),
            );
        }

        let ratio = if max <= 0.0 {
            0.0
        } else {
            (value / max).clamp(0.0, 1.0)
        };
        arc(
            start,
            start + (end - start) * ratio,
            Stroke::new(11.0, color),
        );
        let angle = start + (end - start) * ratio;
        let dir = Vec2::new(angle.cos(), angle.sin());
        let needle_start = center + dir * (radius * 0.45);
        let needle_end = center + dir * (radius * 0.92);
        painter.line_segment(
            [center, needle_start],
            Stroke::new(
                1.0,
                Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 90),
            ),
        );
        painter.line_segment(
            [needle_start, needle_end],
            Stroke::new(5.0, Color32::from_rgb(6, 10, 14)),
        );
        painter.line_segment([needle_start, needle_end], Stroke::new(3.0, color));
        painter.circle_filled(center, 5.0, color);

        painter.text(
            Pos2::new(center.x, center.y - 48.0),
            Align2::CENTER_CENTER,
            format!("{value:.0}"),
            FontId::proportional(32.0),
            Color32::WHITE,
        );
        painter.text(
            Pos2::new(center.x, center.y - 20.0),
            Align2::CENTER_CENTER,
            unit,
            FontId::proportional(12.0),
            Color32::from_gray(160),
        );
        painter.text(
            Pos2::new(rect.center().x, rect.top() + 8.0),
            Align2::CENTER_CENTER,
            label,
            FontId::proportional(13.0),
            Color32::from_gray(190),
        );
        painter.text(
            Pos2::new(rect.center().x, rect.bottom() - 8.0),
            Align2::CENTER_CENTER,
            sub,
            FontId::proportional(11.0),
            Color32::from_gray(145),
        );
        painter.text(
            Pos2::new(rect.right() - 6.0, center.y + radius * 0.55),
            Align2::RIGHT_CENTER,
            "MAX",
            FontId::proportional(9.0),
            Color32::from_gray(130),
        );
    });
}
