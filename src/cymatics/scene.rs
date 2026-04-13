//! Main application: wires together the physics simulation, audio engine,
//! and egui UI into a cohesive interactive experience.

#![allow(dead_code, unused_imports, unused_variables, clippy::cast_precision_loss)]

use egui::{Color32, Painter, Pos2, Rect, Sense};
use std::time::Instant;

use super::audio::AudioEngine;
use super::physics::{self, Particle, PARTICLE_COUNT, STEPS_PER_FRAME};

// ---------------------------------------------------------------------------
// Frequency presets
// ---------------------------------------------------------------------------

struct Preset {
    name:  &'static str,
    freq:  f32,
    /// Brief description shown on hover.
    tip:   &'static str,
}

/// Hand-picked frequencies that each produce a visually distinct, beautiful
/// Chladni pattern under the current freq_to_mode mapping.
const PRESETS: &[Preset] = &[
    Preset { name: "Cross",      freq:  150.0, tip: "Mode (1,2) — simplest two-lobed cross" },
    Preset { name: "Star",       freq:  250.0, tip: "Mode (2,3) — six-pointed star" },
    Preset { name: "Concert A",  freq:  440.0, tip: "Mode (3,4) — 440 Hz, twelve-lobed snowflake" },
    Preset { name: "Lotus",      freq:  528.0, tip: "Mode (3,5) — 528 Hz, fifteen-petal lotus" },
    Preset { name: "Mandala",    freq:  600.0, tip: "Mode (4,5) — twenty-lobed mandala" },
    Preset { name: "Web",        freq:  800.0, tip: "Mode (5,2) — radial spiderweb" },
    Preset { name: "Crystal",    freq:  900.0, tip: "Mode (5,3) — crystalline lattice" },
    Preset { name: "Galaxy",     freq: 1000.0, tip: "Mode (6,4) — spiral galaxy arms" },
    Preset { name: "Cosmos",     freq: 1200.0, tip: "Mode (7,5) — cosmic web" },
    Preset { name: "Fractal",    freq: 1800.0, tip: "Mode (9,5) — maximum complexity" },
];

// ---------------------------------------------------------------------------
// Colour schemes
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ColorScheme {
    /// Classic oscilloscope green on black.
    Phosphor,
    /// Warm sand tones on dark walnut.
    Sand,
    /// Rainbow / neon — speed maps through the hue wheel.
    Neon,
    /// Monochrome white on near-black.
    Mono,
}

impl ColorScheme {
    fn label(self) -> &'static str {
        match self {
            Self::Phosphor => "Phosphor",
            Self::Sand => "Sand",
            Self::Neon => "Neon",
            Self::Mono => "Mono",
        }
    }

    /// Map a particle's speed and displacement to a render colour.
    ///
    /// `speed`  – magnitude of velocity (px/s)
    /// `disp`   – |Z| value (0 = on nodal line, 1 = antinode)
    fn particle_color(self, speed: f32, disp: f32) -> Color32 {
        // Normalise speed: treat 60 px/s as "fast".
        let t = (speed / 60.0).clamp(0.0, 1.0);

        // Near-nodal-line boost: particles sitting on a nodal line glow.
        let on_node = (1.0 - disp.clamp(0.0, 1.0)).powf(3.0);

        match self {
            Self::Phosphor => {
                // Settled: deep teal-green; moving: bright lime.
                let g = lerp(80.0, 255.0, t) as u8;
                let r = lerp(0.0, 80.0, t) as u8;
                let b = lerp(40.0, 20.0, t) as u8;
                let a = lerp(120.0, 220.0, on_node * 0.6 + t * 0.4) as u8;
                Color32::from_rgba_unmultiplied(r, g, b, a)
            }
            Self::Sand => {
                // Settled: burnt umber; moving: bright gold.
                let r = lerp(120.0, 255.0, t) as u8;
                let g = lerp(80.0, 200.0, t) as u8;
                let b = lerp(30.0, 60.0, t) as u8;
                let a = lerp(100.0, 210.0, on_node * 0.5 + t * 0.5) as u8;
                Color32::from_rgba_unmultiplied(r, g, b, a)
            }
            Self::Neon => {
                // Hue rotates with speed; brightness with displacement.
                let hue = t * 300.0; // 0° (red) → 300° (magenta)
                let [r, g, b] = hsv_to_rgb(hue, 1.0, 0.5 + 0.5 * t);
                let a = lerp(90.0, 230.0, on_node * 0.5 + t * 0.5) as u8;
                Color32::from_rgba_unmultiplied(r, g, b, a)
            }
            Self::Mono => {
                let v = lerp(60.0, 255.0, t.max(on_node * 0.4)) as u8;
                let a = lerp(80.0, 200.0, t * 0.6 + on_node * 0.4) as u8;
                Color32::from_rgba_unmultiplied(v, v, v, a)
            }
        }
    }

    /// Background fill colour.
    fn background(self) -> Color32 {
        match self {
            Self::Sand => Color32::from_rgb(18, 12, 8),
            _ => Color32::from_rgb(10, 10, 15),
        }
    }

    /// Nodal-line overlay colour.
    fn nodal_line_color(self) -> Color32 {
        match self {
            Self::Phosphor => Color32::from_rgba_unmultiplied(0, 180, 80, 18),
            Self::Sand => Color32::from_rgba_unmultiplied(200, 160, 80, 18),
            Self::Neon => Color32::from_rgba_unmultiplied(180, 80, 255, 18),
            Self::Mono => Color32::from_rgba_unmultiplied(200, 200, 200, 18),
        }
    }
}

// ---------------------------------------------------------------------------
// App state
// ---------------------------------------------------------------------------

pub struct CymaticsScene {
    // --- Simulation ---
    particles: Vec<Particle>,
    frequency: f32,
    current_m: i32,
    current_n: i32,

    // --- Audio ---
    audio: Option<AudioEngine>,
    volume: f32,

    // --- Visual ---
    color_scheme: ColorScheme,
    show_nodal_lines: bool,

    // --- Steps per frame (user-controlled) ---
    steps_per_frame: usize,

    // --- Timing ---
    last_update: Instant,
    fps_history: [f32; 60],
    fps_index: usize,

    // --- Plate size (set during first paint) ---
    plate_rect: Option<Rect>,
}

impl CymaticsScene {
    pub fn new() -> Self {
        let frequency = 220.0_f32;
        let (m, n) = physics::freq_to_mode(frequency);

        // Allocate particles; positions will be randomised on first frame
        // once we know the plate dimensions.
        let particles = (0..PARTICLE_COUNT)
            .map(|_| Particle::new_random(800.0, 700.0))
            .collect();

        let audio = AudioEngine::try_new();

        Self {
            particles,
            frequency,
            current_m: m,
            current_n: n,
            audio,
            volume: 0.4,
            color_scheme: ColorScheme::Phosphor,
            show_nodal_lines: true,
            steps_per_frame: STEPS_PER_FRAME,
            last_update: Instant::now(),
            fps_history: [60.0; 60],
            fps_index: 0,
            plate_rect: None,
        }
    }

    // -----------------------------------------------------------------------
    // Sync audio state
    // -----------------------------------------------------------------------

    fn sync_audio(&self) {
        if let Some(engine) = &self.audio {
            if let Ok(mut state) = engine.state.lock() {
                state.frequency = self.frequency;
                state.volume = self.volume;
            }
        }
    }

    // -----------------------------------------------------------------------
    // Set frequency (used by both slider and preset buttons)
    // -----------------------------------------------------------------------

    fn set_frequency(&mut self, freq: f32) {
        self.frequency = freq;
        let (m, n) = physics::freq_to_mode(freq);
        if m != self.current_m || n != self.current_n {
            self.current_m = m;
            self.current_n = n;
            if let Some(rect) = self.plate_rect {
                for p in &mut self.particles {
                    p.scatter(rect.width(), rect.height());
                }
            }
        }
        self.sync_audio();
    }

    // -----------------------------------------------------------------------
    // Draw the central plate area
    // -----------------------------------------------------------------------

    fn draw_plate(&mut self, ui: &mut egui::Ui) {
        let available = ui.available_rect_before_wrap();

        // Allocate the full available space and get a response for interaction.
        let (rect, _response) =
            ui.allocate_exact_size(available.size(), Sense::hover());

        // Store plate rect for physics.
        self.plate_rect = Some(rect);

        let painter = ui.painter_at(rect);
        let plate_w = rect.width();
        let plate_h = rect.height();

        // --- Background ---
        painter.rect_filled(rect, 0.0, self.color_scheme.background());

        // --- Nodal line overlay ---
        if self.show_nodal_lines {
            draw_nodal_lines(
                &painter,
                rect,
                self.current_m,
                self.current_n,
                self.color_scheme.nodal_line_color(),
            );
        }

        // --- Particles ---
        for p in &self.particles {
            let speed = p.vel.length();
            let color = self.color_scheme.particle_color(speed, p.displacement);

            let screen_pos = Pos2::new(rect.left() + p.pos.x, rect.top() + p.pos.y);

            // Draw a small filled circle; radius scales slightly with speed.
            let radius = 1.5_f32.max(1.0 + (speed / 80.0).min(1.5));
            painter.circle_filled(screen_pos, radius, color);
        }

        // Mode label (bottom-left corner of plate).
        let mode_text = format!(
            "mode ({}, {})   {:.0} Hz",
            self.current_m, self.current_n, self.frequency
        );
        painter.text(
            Pos2::new(rect.left() + 12.0, rect.bottom() - 16.0),
            egui::Align2::LEFT_BOTTOM,
            &mode_text,
            egui::FontId::proportional(12.0),
            Color32::from_rgba_unmultiplied(180, 180, 180, 90),
        );

        // We need continuous repaints for animation.
        ui.ctx().request_repaint();

        let _ = plate_w;
        let _ = plate_h;
    }

    // -----------------------------------------------------------------------
    // Right-side control panel
    // -----------------------------------------------------------------------

    fn draw_panel(&mut self, ui: &mut egui::Ui) {
        let sec = Color32::from_rgb(180, 185, 205);
        let dim = Color32::from_rgba_unmultiplied(155, 160, 180, 220);
        let hi  = Color32::from_rgb(220, 200, 100);

        ui.add_space(10.0);
        ui.label(
            egui::RichText::new("Cymatics")
                .size(26.0)
                .color(Color32::from_rgb(255, 200, 60))
                .strong(),
        );
        ui.label(
            egui::RichText::new("Chladni Figure Simulator")
                .size(11.0)
                .color(Color32::from_rgb(100, 110, 130)),
        );

        ui.add_space(6.0);
        egui::CollapsingHeader::new(egui::RichText::new("How it works").size(12.0).color(dim))
            .default_open(false)
            .show(ui, |ui| {
                ui.add_space(4.0);
                ui.label(egui::RichText::new("Chladni Figures").size(11.0).color(hi).strong());
                ui.label(egui::RichText::new(
                    "In 1787, Ernst Chladni bowed a metal plate and scattered \
                     sand — the grains collected into geometric patterns now \
                     called Chladni figures.",
                ).size(10.0).color(dim));
                ui.add_space(4.0);
                ui.label(egui::RichText::new("Displacement field").size(11.0).color(hi).strong());
                ui.label(egui::RichText::new(
                    "Z(x,y) = cos(m·π·x/W)·cos(n·π·y/H)\n\
                            − cos(n·π·x/W)·cos(m·π·y/H)\n\n\
                     m, n grow with frequency. Nodal lines are where Z = 0 \
                     — the plate is stationary there.",
                ).size(10.0).color(dim));
                ui.add_space(4.0);
                ui.label(egui::RichText::new("Particles").size(11.0).color(hi).strong());
                ui.label(egui::RichText::new(
                    "3 000 sand grains follow −∇|Z|, migrating toward \
                     nodal lines. Brownian jitter keeps them alive once settled.",
                ).size(10.0).color(dim));
            });

        ui.add_space(8.0);
        ui.separator();
        ui.add_space(8.0);

        // --- Simulation speed ---
        ui.label(
            egui::RichText::new("Simulation Speed")
                .size(13.0)
                .color(sec),
        );
        ui.add(
            egui::Slider::new(&mut self.steps_per_frame, 1..=32)
                .clamping(egui::SliderClamping::Always)
                .custom_formatter(|v, _| {
                    let s = v as usize;
                    if s == 1 { "1× slow".into() }
                    else if s <= 4 { format!("{}× normal", s) }
                    else { format!("{}× fast", s) }
                }),
        );

        ui.add_space(12.0);
        ui.separator();
        ui.add_space(10.0);

        // --- Beautiful frequencies (presets) ---
        ui.label(
            egui::RichText::new("Beautiful Frequencies")
                .size(13.0)
                .color(sec),
        );
        ui.add_space(4.0);

        let btn_w = (ui.available_width() - 8.0) / 2.0;
        let mut chosen_freq: Option<f32> = None;
        egui::Grid::new("presets_grid")
            .num_columns(2)
            .spacing([6.0, 4.0])
            .show(ui, |ui| {
                for (i, preset) in PRESETS.iter().enumerate() {
                    let active = (self.frequency - preset.freq).abs() < 1.0;
                    let btn = egui::Button::new(
                        egui::RichText::new(preset.name).size(12.0)
                    ).selected(active);
                    if ui.add_sized([btn_w, 24.0], btn)
                        .on_hover_text(preset.tip)
                        .clicked()
                    {
                        chosen_freq = Some(preset.freq);
                    }
                    if i % 2 == 1 { ui.end_row(); }
                }
                if PRESETS.len() % 2 == 1 { ui.end_row(); }
            });
        if let Some(f) = chosen_freq {
            self.set_frequency(f);
        }

        ui.add_space(12.0);
        ui.separator();
        ui.add_space(10.0);

        // --- Frequency slider (manual) ---
        ui.label(
            egui::RichText::new("Frequency")
                .size(13.0)
                .color(sec),
        );

        let freq_before = self.frequency;
        ui.add(
            egui::Slider::new(&mut self.frequency, 20.0..=2000.0)
                .clamping(egui::SliderClamping::Always)
                .custom_formatter(|v, _| format!("{:.0} Hz", v)),
        );
        if (self.frequency - freq_before).abs() > 0.5 {
            let f = self.frequency;
            self.set_frequency(f);
        }

        ui.label(
            egui::RichText::new(format!("mode  ({}, {})", self.current_m, self.current_n))
                .size(11.0)
                .color(Color32::from_rgba_unmultiplied(160, 160, 160, 180)),
        );

        ui.add_space(14.0);

        // --- Volume ---
        ui.label(
            egui::RichText::new("Volume")
                .size(13.0)
                .color(sec),
        );

        let vol_percent_before = (self.volume * 100.0) as i32;
        let mut vol_percent = vol_percent_before;
        ui.add(egui::Slider::new(&mut vol_percent, 0..=100).text("%"));
        if vol_percent != vol_percent_before {
            self.volume = vol_percent as f32 / 100.0;
            self.sync_audio();
        }

        ui.add_space(4.0);

        ui.add_space(14.0);
        ui.separator();
        ui.add_space(10.0);

        // --- Scatter button ---
        ui.vertical_centered(|ui| {
            if ui
                .add_sized(
                    [180.0, 36.0],
                    egui::Button::new(
                        egui::RichText::new("  Scatter Particles")
                            .size(14.0)
                            .color(Color32::from_rgb(255, 220, 80)),
                    ),
                )
                .clicked()
            {
                if let Some(rect) = self.plate_rect {
                    for p in &mut self.particles {
                        p.scatter(rect.width(), rect.height());
                    }
                }
            }
        });

        ui.add_space(12.0);
        ui.separator();
        ui.add_space(10.0);

        // --- Color scheme ---
        ui.label(
            egui::RichText::new("Color Scheme")
                .size(13.0)
                .color(sec),
        );

        for scheme in [
            ColorScheme::Phosphor,
            ColorScheme::Sand,
            ColorScheme::Neon,
            ColorScheme::Mono,
        ] {
            let selected = self.color_scheme == scheme;
            if ui
                .selectable_label(selected, egui::RichText::new(scheme.label()).size(13.0))
                .clicked()
            {
                self.color_scheme = scheme;
            }
        }

        ui.add_space(12.0);
        ui.separator();
        ui.add_space(10.0);

        // --- Nodal lines toggle ---
        ui.checkbox(
            &mut self.show_nodal_lines,
            egui::RichText::new("Show Nodal Lines")
                .size(13.0)
                .color(sec),
        );

        ui.add_space(12.0);
        ui.separator();
        ui.add_space(10.0);

        // --- Status ---
        let avg_fps: f32 = self.fps_history.iter().sum::<f32>() / self.fps_history.len() as f32;
        ui.label(
            egui::RichText::new(format!("FPS  {:.0}", avg_fps))
                .size(11.0)
                .color(Color32::from_rgb(90, 140, 90)),
        );
        let audio_label = if self.audio.is_some() {
            egui::RichText::new("Audio  ●  on").size(11.0).color(Color32::from_rgb(90, 200, 120))
        } else {
            egui::RichText::new("Audio  ○  no device").size(11.0).color(Color32::from_rgb(200, 100, 100))
        };
        ui.label(audio_label);
        ui.add_space(8.0);
    }

    pub fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // ---- Timing --------------------------------------------------------
        let now = Instant::now();
        let dt = now.duration_since(self.last_update).as_secs_f32().min(0.05);
        self.last_update = now;

        let fps = if dt > 0.0 { 1.0 / dt } else { 60.0 };
        self.fps_history[self.fps_index] = fps;
        self.fps_index = (self.fps_index + 1) % self.fps_history.len();

        // ---- Physics step (multiple sub-steps for stable integration) ------
        if let Some(rect) = self.plate_rect {
            let sub_dt = dt / self.steps_per_frame as f32;
            for _ in 0..self.steps_per_frame {
                physics::update(
                    &mut self.particles,
                    self.current_m,
                    self.current_n,
                    sub_dt,
                    rect.width(),
                    rect.height(),
                );
            }
        }

        // ---- Layout --------------------------------------------------------
        egui::SidePanel::right("controls")
            .min_width(260.0)
            .max_width(300.0)
            .resizable(false)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    self.draw_panel(ui);
                });
            });

        egui::CentralPanel::default()
            .frame(egui::Frame::none())
            .show(ctx, |ui| {
                self.draw_plate(ui);
            });
    }
}

// ---------------------------------------------------------------------------
// Helpers: nodal line rasterisation
// ---------------------------------------------------------------------------

/// Draw faint nodal line contours by sampling Z on a coarse grid and
/// drawing small dots where |Z| is near zero.
fn draw_nodal_lines(
    painter: &Painter,
    rect: Rect,
    m: i32,
    n: i32,
    color: Color32,
) {
    let w = rect.width();
    let h = rect.height();
    let step = 6.0_f32; // grid resolution (pixels)
    let threshold = 0.08_f32;

    let cols = (w / step) as i32;
    let rows = (h / step) as i32;

    for row in 0..=rows {
        for col in 0..=cols {
            let px = col as f32 * step;
            let py = row as f32 * step;
            let z = physics::chladni_z(px, py, m, n, w, h).abs();
            if z < threshold {
                // Intensity proportional to how close to zero.
                let alpha_f = (1.0 - z / threshold).powf(2.0);
                let a = (color.a() as f32 * alpha_f) as u8;
                let c = Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), a);
                painter.circle_filled(
                    Pos2::new(rect.left() + px, rect.top() + py),
                    1.5,
                    c,
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Colour math utilities
// ---------------------------------------------------------------------------

#[inline]
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t.clamp(0.0, 1.0)
}

/// Convert HSV (h in degrees 0-360, s and v in 0-1) to RGB bytes.
fn hsv_to_rgb(h: f32, s: f32, v: f32) -> [u8; 3] {
    let h = h.rem_euclid(360.0);
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0).rem_euclid(2.0) - 1.0).abs());
    let m = v - c;

    let (r1, g1, b1) = if h < 60.0 {
        (c, x, 0.0)
    } else if h < 120.0 {
        (x, c, 0.0)
    } else if h < 180.0 {
        (0.0, c, x)
    } else if h < 240.0 {
        (0.0, x, c)
    } else if h < 300.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };

    [
        ((r1 + m) * 255.0) as u8,
        ((g1 + m) * 255.0) as u8,
        ((b1 + m) * 255.0) as u8,
    ]
}
