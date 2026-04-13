#![allow(dead_code, unused_imports, unused_variables, clippy::cast_precision_loss)]

use egui::{
    epaint::{Mesh, Vertex},
    Color32, Painter, Pos2, Rect, Rgba, Sense, Stroke, Vec2,
};
use std::sync::{Arc, Mutex};

use super::audio::{AudioEngine, AudioState, Waveform};

// ─── constants ────────────────────────────────────────────────────────────────

const MIN_FREQ: f32 = 110.0;
const MAX_FREQ: f32 = 1760.0;
const TRAIL_LIFE_DEFAULT: f32 = 4.0;
const TRAIL_WIDTH_DEFAULT: f32 = 3.0;

// ─── Trail point ──────────────────────────────────────────────────────────────

struct TrailPoint {
    pos: Pos2,
    freq: f32,
    vol: f32,
    age: f32,
}

// ─── App ──────────────────────────────────────────────────────────────────────

pub struct ThereminScene {
    audio: AudioEngine,

    trail: Vec<TrailPoint>,
    trail_life: f32,
    trail_width: f32,
    master_volume: f32,

    last_time: f64,
    pulse_t: f32,

    current_freq: f32,
    current_vol: f32,
    mouse_active: bool,
    waveform: Waveform,
}

impl ThereminScene {
    pub fn new() -> Self {
        Self {
            audio: AudioEngine::new(),
            trail: Vec::new(),
            trail_life: TRAIL_LIFE_DEFAULT,
            trail_width: TRAIL_WIDTH_DEFAULT,
            master_volume: 1.0,
            last_time: 0.0,
            pulse_t: 0.0,
            current_freq: MIN_FREQ,
            current_vol: 0.0,
            mouse_active: false,
            waveform: Waveform::Sine,
        }
    }

    pub fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // ── Delta time ──
        let now = ctx.input(|i| i.time);
        let dt = if self.last_time == 0.0 {
            0.016
        } else {
            (now - self.last_time).min(0.1) as f32
        };
        self.last_time = now;
        self.pulse_t += dt * 3.0;

        // ── Right-side panel ──
        egui::SidePanel::right("controls")
            .min_width(240.0)
            .max_width(300.0)
            .show(ctx, |ui| {
                let sec = Color32::from_rgb(180, 185, 205);
                let dim = Color32::from_rgba_unmultiplied(155, 160, 180, 220);
                let hi  = Color32::from_rgb(180, 150, 255);

                ui.add_space(10.0);
                ui.label(
                    egui::RichText::new("Theremin")
                        .size(26.0)
                        .color(Color32::from_rgb(200, 180, 255))
                        .strong(),
                );
                ui.label(
                    egui::RichText::new("X = pitch  •  Y = volume")
                        .size(11.0)
                        .color(Color32::from_rgb(100, 110, 130)),
                );

                ui.add_space(6.0);
                egui::CollapsingHeader::new(egui::RichText::new("How it works").size(12.0).color(dim))
                    .default_open(false)
                    .show(ui, |ui| {
                        ui.add_space(4.0);
                        ui.label(egui::RichText::new("The Theremin").size(11.0).color(hi).strong());
                        ui.label(egui::RichText::new(
                            "Invented by Léon Theremin in 1920 — the first \
                             electronic instrument played without touch. Two \
                             antennas sense hand proximity for pitch and volume.",
                        ).size(10.0).color(dim));
                        ui.add_space(4.0);
                        ui.label(egui::RichText::new("X axis — Pitch").size(11.0).color(hi).strong());
                        ui.label(egui::RichText::new(
                            "Exponential (log) scale: freq = 110 × (1760/110)^(x/W)\n\
                             4 octaves A2 → A6, matching how we perceive pitch.",
                        ).size(10.0).color(dim));
                        ui.add_space(4.0);
                        ui.label(egui::RichText::new("Y axis — Volume").size(11.0).color(hi).strong());
                        ui.label(egui::RichText::new(
                            "Top = loud, bottom = quiet. Hover = 40% volume; \
                             hold click = full volume.",
                        ).size(10.0).color(dim));
                    });

                ui.add_space(8.0);
                ui.separator();

                // Current note
                ui.add_space(8.0);
                let note = freq_to_note(self.current_freq);
                let hue = freq_to_hue(self.current_freq);
                let note_col = hue_to_color(hue, 0.8, 1.0, 255);
                ui.horizontal(|ui| {
                    ui.label("Note:");
                    ui.label(
                        egui::RichText::new(&note)
                            .size(28.0)
                            .strong()
                            .color(note_col),
                    );
                });
                ui.label(format!("{:.1} Hz", self.current_freq));

                ui.add_space(10.0);
                ui.separator();

                // Waveform selector
                ui.add_space(6.0);
                ui.label(egui::RichText::new("Waveform").size(13.0).color(sec));
                ui.horizontal(|ui| {
                    let selected = self.waveform;
                    if ui
                        .selectable_label(selected == Waveform::Sine, "Sine")
                        .clicked()
                    {
                        self.waveform = Waveform::Sine;
                    }
                    if ui
                        .selectable_label(selected == Waveform::Sawtooth, "Saw")
                        .clicked()
                    {
                        self.waveform = Waveform::Sawtooth;
                    }
                    if ui
                        .selectable_label(selected == Waveform::Square, "Square")
                        .clicked()
                    {
                        self.waveform = Waveform::Square;
                    }
                });

                ui.add_space(10.0);
                ui.separator();

                // Trail life slider
                ui.add_space(6.0);
                ui.label(egui::RichText::new("Trail life").size(13.0).color(sec));
                ui.add(
                    egui::Slider::new(&mut self.trail_life, 1.0..=8.0)
                        .show_value(true)
                        .suffix("s"),
                );

                // Trail width slider
                ui.add_space(4.0);
                ui.label(egui::RichText::new("Trail width").size(13.0).color(sec));
                ui.add(
                    egui::Slider::new(&mut self.trail_width, 1.0..=8.0)
                        .show_value(true)
                        .suffix("px"),
                );

                // Clear button
                ui.add_space(8.0);
                if ui
                    .add(
                        egui::Button::new("Clear Trails")
                            .fill(Color32::from_rgb(60, 40, 80)),
                    )
                    .clicked()
                {
                    self.trail.clear();
                }

                ui.add_space(10.0);
                ui.separator();

                // Master volume
                ui.add_space(6.0);
                ui.label(egui::RichText::new("Volume").size(13.0).color(sec));
                ui.add(
                    egui::Slider::new(&mut self.master_volume, 0.0..=1.0)
                        .show_value(true),
                );

                ui.add_space(10.0);
                ui.separator();

                // FPS
                ui.add_space(6.0);
                let fps = ctx.input(|i| i.unstable_dt).recip();
                ui.label(
                    egui::RichText::new(format!("FPS  {:.0}", fps.min(9999.0)))
                        .size(11.0)
                        .color(Color32::from_rgb(90, 140, 90)),
                );

                ui.add_space(4.0);
                if !self.mouse_active {
                    ui.label(
                        egui::RichText::new("Move mouse over canvas")
                            .color(Color32::from_rgb(140, 140, 140))
                            .italics(),
                    );
                }

            });

        // ── Central canvas panel ──
        let bg = egui::Frame::default().fill(Color32::BLACK);
        egui::CentralPanel::default().frame(bg).show(ctx, |ui| {
            let available = ui.available_rect_before_wrap();
            let response = ui.allocate_rect(available, Sense::hover());
            let painter = ui.painter_at(available);
            let rect = available;

            // Grid
            render_grid(&painter, rect);

            // Mouse / pointer input
            let pointer = ctx.input(|i| {
                (
                    i.pointer.latest_pos(),
                    i.pointer.primary_down(),
                    i.pointer.is_moving(),
                )
            });

            let (mouse_pos, primary_down, is_moving) = pointer;

            let over_canvas = mouse_pos
                .map(|p| rect.contains(p))
                .unwrap_or(false);

            self.mouse_active = over_canvas && mouse_pos.is_some();

            if self.mouse_active {
                let pos = mouse_pos.unwrap();
                let rel_x = (pos.x - rect.left()) / rect.width();
                let rel_y = (pos.y - rect.top()) / rect.height();

                let freq =
                    MIN_FREQ * (MAX_FREQ / MIN_FREQ).powf(rel_x.clamp(0.0, 1.0));
                // Y=0 top → 100%, Y=bottom → 10%
                let vol_raw = 1.0 - rel_y.clamp(0.0, 1.0) * 0.9;
                // Clicking = full vol, hovering = 40%
                let vol = if primary_down {
                    vol_raw
                } else {
                    vol_raw * 0.4
                } * self.master_volume;

                self.current_freq = freq;
                self.current_vol = vol;

                // Push trail point
                self.trail.push(TrailPoint {
                    pos,
                    freq,
                    vol: vol_raw, // store raw for color weight
                    age: 0.0,
                });

                // Update audio
                if let Ok(mut s) = self.audio.state.try_lock() {
                    s.target_freq = freq;
                    s.target_vol = vol;
                    s.active = true;
                    s.waveform = self.waveform;
                }
            } else {
                // Silence
                if let Ok(mut s) = self.audio.state.try_lock() {
                    s.active = false;
                    s.target_vol = 0.0;
                }
            }

            // Age trail points and cull old ones
            for p in &mut self.trail {
                p.age += dt;
            }
            self.trail.retain(|p| p.age < self.trail_life);

            // Render trail
            render_trail(&painter, &self.trail, self.trail_life, self.trail_width);

            // Pulsing cursor circle
            if self.mouse_active {
                if let Some(pos) = mouse_pos {
                    let pulse = (self.pulse_t.sin() * 0.5 + 0.5) * 0.4 + 0.6;
                    let hue = freq_to_hue(self.current_freq);
                    let col = hue_to_color(hue, 0.9, 1.0, (pulse * 200.0) as u8);
                    let r = 8.0 * pulse;
                    painter.circle_filled(pos, r, col);
                    painter.circle_stroke(
                        pos,
                        r + 4.0,
                        Stroke::new(1.5, hue_to_color(hue, 0.6, 1.0, (pulse * 100.0) as u8)),
                    );
                }
            }
        });

        // Request continuous repaint
        ctx.request_repaint();
    }
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

/// Map frequency to hue [0, 0.75]  (red = low, violet = high)
fn freq_to_hue(freq: f32) -> f32 {
    let t = ((freq / MIN_FREQ).log2() / (MAX_FREQ / MIN_FREQ).log2()).clamp(0.0, 1.0);
    t * 0.75
}

/// HSV → Color32 (v and s = 1)
fn hue_to_color(hue: f32, sat: f32, val: f32, alpha: u8) -> Color32 {
    let h = hue.fract() * 6.0;
    let i = h.floor() as u32;
    let f = h - h.floor();
    let p = val * (1.0 - sat);
    let q = val * (1.0 - sat * f);
    let t = val * (1.0 - sat * (1.0 - f));
    let (r, g, b) = match i % 6 {
        0 => (val, t, p),
        1 => (q, val, p),
        2 => (p, val, t),
        3 => (p, q, val),
        4 => (t, p, val),
        _ => (val, p, q),
    };
    Color32::from_rgba_unmultiplied(
        (r * 255.0) as u8,
        (g * 255.0) as u8,
        (b * 255.0) as u8,
        alpha,
    )
}

/// freq → note name (e.g. "A4", "C#3")
fn freq_to_note(freq: f32) -> String {
    let note_names = ["A", "A#", "B", "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#"];
    // A4 = 440 Hz, MIDI note 69
    let midi = 69.0 + 12.0 * (freq / 440.0).log2();
    let midi_round = midi.round() as i32;
    // Note index relative to A
    let note_idx = ((midi_round % 12) + 12) as usize % 12;
    // Octave: MIDI 0 = C-1; A0 = MIDI 21
    let octave = (midi_round + 9) / 12 - 1;
    format!("{}{}", note_names[note_idx], octave)
}

/// Draw a thick line segment as a quad strip into `mesh`
fn push_segment(mesh: &mut Mesh, a: Pos2, b: Pos2, half_w: f32, col: Color32) {
    let dir = (b - a).normalized();
    let perp = Vec2::new(-dir.y, dir.x) * half_w;

    let base = mesh.vertices.len() as u32;
    mesh.vertices.push(Vertex {
        pos: a + perp,
        uv: Pos2::ZERO,
        color: col,
    });
    mesh.vertices.push(Vertex {
        pos: a - perp,
        uv: Pos2::ZERO,
        color: col,
    });
    mesh.vertices.push(Vertex {
        pos: b + perp,
        uv: Pos2::ZERO,
        color: col,
    });
    mesh.vertices.push(Vertex {
        pos: b - perp,
        uv: Pos2::ZERO,
        color: col,
    });
    // Two triangles
    mesh.indices
        .extend_from_slice(&[base, base + 1, base + 2, base + 1, base + 3, base + 2]);
}

/// Render the trail with 3-pass glow
fn render_trail(painter: &Painter, trail: &[TrailPoint], trail_life: f32, base_width: f32) {
    if trail.len() < 2 {
        return;
    }

    // 3 passes: outer glow, mid glow, core
    let passes: &[(f32, f32)] = &[(3.0, 0.15), (1.5, 0.40), (0.6, 1.0)];

    for &(width_mult, alpha_mult) in passes {
        let mut mesh = Mesh::default();

        for i in 0..trail.len() - 1 {
            let p0 = &trail[i];
            let p1 = &trail[i + 1];

            let hue = freq_to_hue(p0.freq);
            // Alpha fades with age
            let age_t = (p0.age / trail_life).clamp(0.0, 1.0);
            let base_alpha = (1.0 - age_t).powf(1.5) * p0.vol;
            let alpha = ((base_alpha * alpha_mult).clamp(0.0, 1.0) * 255.0) as u8;

            if alpha < 2 {
                continue;
            }

            let line_width = (base_width * p0.vol + 2.0) * width_mult;
            let col = hue_to_color(hue, 1.0, 1.0, alpha);

            push_segment(&mut mesh, p0.pos, p1.pos, line_width * 0.5, col);
        }

        if !mesh.vertices.is_empty() {
            painter.add(egui::Shape::mesh(mesh));
        }
    }
}

/// Draw the frequency/volume grid
fn render_grid(painter: &Painter, rect: Rect) {
    let grid_col = Color32::from_rgba_unmultiplied(80, 80, 80, 60);
    let label_col = Color32::from_rgba_unmultiplied(120, 120, 120, 90);
    let w = rect.width();
    let h = rect.height();

    // Vertical lines at octave frequencies
    let octave_freqs = [110.0f32, 220.0, 440.0, 880.0, 1760.0];
    let octave_labels = ["A2", "A3", "A4", "A5", "A6"];
    for (freq, label) in octave_freqs.iter().zip(octave_labels.iter()) {
        let t = ((*freq / MIN_FREQ).log2() / (MAX_FREQ / MIN_FREQ).log2()).clamp(0.0, 1.0);
        let x = rect.left() + t * w;
        painter.line_segment(
            [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
            Stroke::new(1.0, grid_col),
        );
        painter.text(
            Pos2::new(x + 3.0, rect.top() + 4.0),
            egui::Align2::LEFT_TOP,
            label,
            egui::FontId::proportional(10.0),
            label_col,
        );
    }

    // Horizontal lines at 25%, 50%, 75% volume
    for vol_frac in [0.25f32, 0.50, 0.75] {
        // vol = 1 - y/h => y = (1 - vol) * h
        let y = rect.top() + (1.0 - vol_frac) * h;
        painter.line_segment(
            [Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)],
            Stroke::new(1.0, grid_col),
        );
    }
}
