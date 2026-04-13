//! Main application: UI layout, canvas rendering, interaction handling.
#![allow(dead_code, unused_variables, unused_imports, clippy::cast_precision_loss)]

use std::sync::{Arc, Mutex};
use std::time::Instant;

use egui::{Color32, Pos2, Rect, Response, Sense, Stroke, Ui, Vec2 as EguiVec2};
use glam::Vec2;

use super::audio::{AudioEngine, AudioState, ToneDesc};
use super::physics::{self, mass_to_freq, Planet, Particle, PARTICLE_COUNT};

// ─── Constants ────────────────────────────────────────────────────────────────

/// Maximum planets on screen. The 7th click evicts the oldest.
const MAX_PLANETS: usize = 6;

/// Golden-ratio hue step — gives maximally distinct colours for any count.
const GOLDEN_HUE: f32 = 0.618033988;

/// Time window (seconds) for registering a double-click.
const DOUBLE_CLICK_SECS: f64 = 0.35;

/// Pixel radius within which a click is considered "on" a planet.
const HIT_RADIUS: f32 = 22.0;

// ─── Colour helpers ───────────────────────────────────────────────────────────

fn hsv_to_color32(h: f32, s: f32, v: f32, a: u8) -> Color32 {
    let h6 = h * 6.0;
    let i  = h6.floor() as i32 % 6;
    let f  = h6 - h6.floor();
    let p  = v * (1.0 - s);
    let q  = v * (1.0 - f * s);
    let t  = v * (1.0 - (1.0 - f) * s);
    let (r, g, b) = match i {
        0 => (v, t, p), 1 => (q, v, p), 2 => (p, v, t),
        3 => (p, q, v), 4 => (t, p, v), _ => (v, p, q),
    };
    Color32::from_rgba_unmultiplied(
        (r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8, a,
    )
}

fn particle_color(hue: f32, speed: f32, alpha: f32) -> Color32 {
    let t = (speed / 400.0).clamp(0.0, 1.0);
    hsv_to_color32((hue + t * 0.15) % 1.0, 0.8 + t * 0.2, 0.4 + t * 0.6, (alpha * 255.0) as u8)
}

fn hue_color(hue: f32, alpha: f32) -> Color32 {
    hsv_to_color32(hue, 0.9, 1.0, (alpha * 255.0) as u8)
}

// ─── GravityScene ─────────────────────────────────────────────────────────────

pub struct GravityScene {
    planets:          Vec<Planet>,
    particles:        Vec<Particle>,
    audio:            Option<AudioEngine>,
    audio_state:      Arc<Mutex<AudioState>>,

    dragging_planet:  Option<usize>,

    trail_opacity:    f32,
    show_planets:     bool,
    steps_per_frame:  usize,
    canvas_rect:      Rect,

    fps:              f64,
    last_frame:       Instant,

    /// Monotonically-increasing counter → distinct hues via golden ratio.
    hue_counter:      u32,

    /// Tracks the last click on a planet for double-click detection.
    /// `(timestamp_secs, planet_index)`
    last_planet_click: Option<(f64, usize)>,
}

impl GravityScene {
    pub fn new() -> Self {
        let audio_state = Arc::new(Mutex::new(AudioState::new()));
        let audio       = AudioEngine::try_new(Arc::clone(&audio_state));

        let mut hue_counter = 0u32;
        let h0 = alloc_hue(&mut hue_counter);
        let h1 = alloc_hue(&mut hue_counter);

        let planets = vec![
            Planet::new(Vec2::new(380.0, 380.0), 2000.0, h0),
            Planet::new(Vec2::new(720.0, 380.0), 1200.0, h1),
        ];
        let particles = physics::seed_particles(&planets);

        Self {
            planets, particles, audio, audio_state,
            dragging_planet: None,
            trail_opacity: 0.7,
            show_planets: true,
            steps_per_frame: 2,
            canvas_rect: Rect::EVERYTHING,
            fps: 60.0,
            last_frame: Instant::now(),
            hue_counter,
            last_planet_click: None,
        }
    }

    // ── Physics ───────────────────────────────────────────────────────────────

    fn update_physics(&mut self, dt: f32) {
        let bounds = Vec2::new(self.canvas_rect.width(), self.canvas_rect.height());
        for p in self.particles.iter_mut() {
            if p.trail.is_empty() && !self.planets.is_empty() {
                let idx = fastrand::usize(..self.planets.len());
                physics::spawn_orbit(p, &self.planets[idx]);
            }
        }
        let sub_dt = dt / self.steps_per_frame as f32;
        for _ in 0..self.steps_per_frame {
            physics::step(&mut self.particles, &self.planets, sub_dt, bounds);
        }
    }

    // ── Audio ─────────────────────────────────────────────────────────────────

    fn update_audio(&self) {
        let tones: Vec<ToneDesc> = self.planets.iter()
            .map(|pl| ToneDesc { frequency: pl.freq, amplitude: 1.0 })
            .collect();
        if let Ok(mut st) = self.audio_state.try_lock() {
            st.set_tones(tones);
        }
    }

    // ── Planet management ─────────────────────────────────────────────────────

    fn add_planet_at(&mut self, pos: Vec2) {
        // If at max, evict the oldest (index 0).
        if self.planets.len() >= MAX_PLANETS {
            self.planets.remove(0);
        }
        let hue = alloc_hue(&mut self.hue_counter);
        let mass = 500.0 + fastrand::f32() * 4500.0;
        self.planets.push(Planet::new(pos, mass, hue));
        self.redistrib_particles();
    }

    fn add_random_planet(&mut self) {
        let w = self.canvas_rect.width().max(200.0);
        let h = self.canvas_rect.height().max(200.0);
        let x = 50.0 + fastrand::f32() * (w - 100.0);
        let y = 50.0 + fastrand::f32() * (h - 100.0);
        self.add_planet_at(Vec2::new(x, y));
    }

    fn remove_planet(&mut self, idx: usize) {
        self.planets.remove(idx);
        self.last_planet_click = None;
        if !self.planets.is_empty() {
            self.redistrib_particles();
        }
    }

    fn redistrib_particles(&mut self) {
        if self.planets.is_empty() { return; }
        for (i, p) in self.particles.iter_mut().enumerate() {
            physics::spawn_orbit(p, &self.planets[i % self.planets.len()]);
        }
    }

    /// Find the index of a planet whose screen position is within HIT_RADIUS of `screen_pos`.
    fn planet_near_screen(&self, screen_pos: Pos2) -> Option<usize> {
        self.planets.iter().position(|pl| {
            let sp = canvas_to_screen(self.canvas_rect, pl.pos);
            (sp - screen_pos).length() < HIT_RADIUS.max(pl.radius() + 4.0)
        })
    }

    // ── Side panel ────────────────────────────────────────────────────────────

    fn draw_panel(&mut self, ui: &mut Ui, now_secs: f64) {
        let sec = Color32::from_rgb(180, 185, 205);
        let dim = Color32::from_rgba_unmultiplied(155, 160, 180, 220);
        let hi  = Color32::from_rgb(160, 190, 255);

        ui.add_space(10.0);
        ui.label(
            egui::RichText::new("Gravity Wells")
                .size(26.0).color(Color32::from_rgb(180, 200, 255)).strong(),
        );
        ui.label(
            egui::RichText::new("Click canvas to add  •  Double-click to remove")
                .size(11.0).color(Color32::from_rgb(100, 110, 130)),
        );

        ui.add_space(6.0);
        egui::CollapsingHeader::new(egui::RichText::new("How it works").size(12.0).color(dim))
            .default_open(false)
            .show(ui, |ui| {
                ui.add_space(4.0);
                ui.label(egui::RichText::new("N-body gravity").size(11.0).color(hi).strong());
                ui.label(egui::RichText::new(
                    "Each planet attracts every particle via Newton's \
                     inverse-square law: a = G·M / r²  — more massive \
                     planets pull harder and from farther away.",
                ).size(10.0).color(dim));
                ui.add_space(4.0);
                ui.label(egui::RichText::new("Sound").size(11.0).color(hi).strong());
                ui.label(egui::RichText::new(
                    "Each planet emits a sine tone. Heavier planets \
                     rumble lower. All tones play together as a \
                     gravitational chord.",
                ).size(10.0).color(dim));
                ui.add_space(4.0);
                ui.label(egui::RichText::new("Colors").size(11.0).color(hi).strong());
                ui.label(egui::RichText::new(
                    "Hues are allocated by the golden-ratio step \
                     (Δh ≈ 0.618) so each new planet is always \
                     maximally distinct from the others.",
                ).size(10.0).color(dim));
            });

        ui.add_space(8.0);
        ui.separator();
        ui.add_space(6.0);

        // ── Per-planet controls ───────────────────────────────────────────────
        ui.label(egui::RichText::new("Planets").size(13.0).color(sec));
        ui.add_space(4.0);

        let mut to_remove: Option<usize> = None;
        let mut mass_changed = vec![false; self.planets.len()];

        for i in 0..self.planets.len() {
            let hue  = self.planets[i].hue;
            let freq = self.planets[i].freq;

            egui::Frame::none()
                .fill(Color32::from_rgba_unmultiplied(20, 22, 40, 200))
                .rounding(6.0)
                .inner_margin(egui::Margin::same(6.0))
                .show(ui, |ui| {
                    // Header row: dot + freq label + remove button
                    ui.horizontal(|ui| {
                        let (rect, _) = ui.allocate_exact_size(EguiVec2::splat(14.0), Sense::hover());
                        ui.painter().circle_filled(rect.center(), 7.0, hue_color(hue, 1.0));

                        ui.label(
                            egui::RichText::new(format!("{:.0} Hz", freq))
                                .color(hue_color(hue, 1.0)),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button(
                                egui::RichText::new("×").color(Color32::from_rgb(200, 80, 80))
                            ).clicked() {
                                to_remove = Some(i);
                            }
                        });
                    });

                    // Mass / gravity slider
                    ui.label(
                        egui::RichText::new("Gravity / Mass")
                            .size(11.0).color(Color32::from_rgb(140, 150, 170)),
                    );
                    let resp = ui.add(
                        egui::Slider::new(&mut self.planets[i].mass, 200.0..=8000.0)
                            .show_value(true)
                            .fixed_decimals(0)
                            .suffix(" M"),
                    );
                    if resp.changed() {
                        mass_changed[i] = true;
                    }
                });

            ui.add_space(4.0);
        }

        // Update freq for any planets whose mass was changed.
        for (i, changed) in mass_changed.iter().enumerate() {
            if *changed {
                self.planets[i].freq = mass_to_freq(self.planets[i].mass);
            }
        }

        if let Some(idx) = to_remove {
            self.remove_planet(idx);
        }

        if self.planets.is_empty() {
            ui.label(
                egui::RichText::new("No planets — click the canvas!")
                    .italics().color(Color32::from_rgb(90, 100, 120)),
            );
        }

        ui.add_space(8.0);
        ui.separator();
        ui.add_space(6.0);

        // ── Global sliders ────────────────────────────────────────────────────
        ui.label(egui::RichText::new("Trail Opacity").color(sec));
        ui.add(egui::Slider::new(&mut self.trail_opacity, 0.1..=1.0));

        ui.add_space(4.0);
        ui.label(egui::RichText::new("Steps / Frame").color(sec));
        ui.add(egui::Slider::new(&mut self.steps_per_frame, 1..=8));

        ui.add_space(4.0);
        ui.checkbox(
            &mut self.show_planets,
            egui::RichText::new("Show Planets").color(sec),
        );

        ui.add_space(8.0);
        ui.separator();
        ui.add_space(6.0);

        ui.horizontal(|ui| {
            if ui.button(egui::RichText::new("Add Random").color(Color32::from_rgb(100, 220, 150))).clicked() {
                self.add_random_planet();
            }
            if ui.button(egui::RichText::new("Clear All").color(Color32::from_rgb(220, 120, 80))).clicked() {
                self.planets.clear();
                for p in self.particles.iter_mut() { p.trail.clear(); }
                self.last_planet_click = None;
            }
        });

        ui.add_space(10.0);
        ui.separator();
        ui.add_space(4.0);

        ui.label(
            egui::RichText::new(format!("{:.1} fps  •  {} particles", self.fps, PARTICLE_COUNT))
                .size(11.0).color(Color32::from_rgb(90, 140, 90)),
        );
    }

    // ── Canvas ────────────────────────────────────────────────────────────────

    fn draw_canvas(&mut self, ui: &mut Ui, now_secs: f64) {
        let available = ui.available_rect_before_wrap();
        self.canvas_rect = available;

        let (response, painter) = ui.allocate_painter(available.size(), Sense::click_and_drag());
        painter.rect_filled(available, 0.0, Color32::from_rgb(5, 5, 15));

        // ── Particle trails ───────────────────────────────────────────────────
        for particle in &self.particles {
            if particle.trail.len() < 2 { continue; }
            let hue        = closest_planet_hue(&self.planets, particle.pos);
            let speed      = particle.vel.length();
            let trail_len  = particle.trail.len();
            let trail_vec: Vec<Vec2> = particle.trail.iter().copied().collect();
            for (seg_idx, seg) in trail_vec.windows(2).enumerate() {
                let t     = (seg_idx + 1) as f32 / trail_len as f32;
                let alpha = t * self.trail_opacity;
                if alpha < 0.01 { continue; }
                let color = particle_color(hue, speed * t, alpha);
                let p0 = canvas_to_screen(available, seg[0]);
                let p1 = canvas_to_screen(available, seg[1]);
                painter.line_segment([p0, p1], Stroke::new(1.5, color));
            }
        }

        // ── Planets ───────────────────────────────────────────────────────────
        if self.show_planets {
            for planet in &self.planets {
                let center = canvas_to_screen(available, planet.pos);
                let r      = planet.radius();
                painter.circle_filled(center, r * 2.5, hsv_to_color32(planet.hue, 0.7, 0.9, 30));
                painter.circle_filled(center, r * 1.5, hsv_to_color32(planet.hue, 0.8, 1.0, 80));
                painter.circle_filled(center, r,       hue_color(planet.hue, 1.0));
            }
        }

        // ── Interaction ───────────────────────────────────────────────────────
        self.handle_canvas_input(&response, available, now_secs);
    }

    fn handle_canvas_input(&mut self, response: &Response, rect: Rect, now_secs: f64) {
        let pointer = response.ctx.input(|i| i.pointer.clone());

        // ── Drag: move planet ─────────────────────────────────────────────────
        if response.drag_started() {
            if let Some(pos) = pointer.interact_pos() {
                self.dragging_planet = self.planet_near_screen(pos);
            }
        }
        if response.dragged() {
            if let Some(idx) = self.dragging_planet {
                if let Some(pos) = pointer.interact_pos() {
                    self.planets[idx].pos = screen_to_canvas(rect, pos);
                }
            }
        }
        if response.drag_stopped() {
            self.dragging_planet = None;
        }

        // ── Click: add planet or double-click to remove ───────────────────────
        if response.clicked() {
            if let Some(pos) = pointer.interact_pos() {
                if let Some(idx) = self.planet_near_screen(pos) {
                    // Click on an existing planet — check for double-click.
                    let is_double = self.last_planet_click
                        .map(|(t, i)| i == idx && (now_secs - t) < DOUBLE_CLICK_SECS)
                        .unwrap_or(false);

                    if is_double {
                        self.remove_planet(idx);
                    } else {
                        self.last_planet_click = Some((now_secs, idx));
                    }
                } else {
                    // Click on empty canvas → add planet.
                    self.last_planet_click = None;
                    let world = screen_to_canvas(rect, pos);
                    self.add_planet_at(world);
                }
            }
        }
    }

    pub fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let now_inst = Instant::now();
        let dt_real  = now_inst.duration_since(self.last_frame).as_secs_f64();
        self.last_frame = now_inst;
        self.fps = self.fps * 0.9 + (1.0 / dt_real.max(0.001)) * 0.1;

        let dt = (dt_real as f32).clamp(0.001, 0.033);
        self.update_physics(dt);
        self.update_audio();

        let now_secs = ctx.input(|i| i.time);

        egui::SidePanel::right("controls")
            .min_width(270.0)
            .max_width(340.0)
            .resizable(false)
            .frame(
                egui::Frame::default()
                    .fill(Color32::from_rgb(10, 12, 22))
                    .inner_margin(egui::Margin::same(12.0)),
            )
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    self.draw_panel(ui, now_secs);
                });
            });

        egui::CentralPanel::default()
            .frame(egui::Frame::default().fill(Color32::from_rgb(5, 5, 15)))
            .show(ctx, |ui| {
                self.draw_canvas(ui, now_secs);
            });

        ctx.request_repaint();
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Allocate the next hue using golden-ratio stepping for maximum distinction.
fn alloc_hue(counter: &mut u32) -> f32 {
    let h = (*counter as f32 * GOLDEN_HUE) % 1.0;
    *counter += 1;
    h
}

#[inline]
fn canvas_to_screen(rect: Rect, pos: Vec2) -> Pos2 {
    Pos2::new(rect.left() + pos.x, rect.top() + pos.y)
}

#[inline]
fn screen_to_canvas(rect: Rect, pos: Pos2) -> Vec2 {
    Vec2::new(pos.x - rect.left(), pos.y - rect.top())
}

fn closest_planet_hue(planets: &[Planet], pos: Vec2) -> f32 {
    if planets.is_empty() { return 0.6; }
    planets.iter()
        .min_by(|a, b| {
            (a.pos - pos).length().partial_cmp(&(b.pos - pos).length()).unwrap()
        })
        .map(|pl| pl.hue)
        .unwrap_or(0.6)
}
