#![allow(dead_code, unused_imports, unused_variables, clippy::cast_precision_loss)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use egui::{TextureHandle, TextureOptions, Pos2, Vec2, Color32, RichText, Rect};
use cpal::Stream;

use super::membrane::{Membrane, ColorMode, WaveMode, GRID_W, GRID_H};
use super::audio::{AudioState, DrumType, setup_audio};

// ─── Per-touch tracking ───────────────────────────────────────────────────────

struct TouchPoint {
    last_pos:  Pos2,
    start_pos: Pos2,
    is_drag:   bool,
}

// ─── Scene ────────────────────────────────────────────────────────────────────

pub struct FluidDrumScene {
    membrane:   Membrane,
    texture:    Option<TextureHandle>,

    audio_state: Arc<Mutex<AudioState>>,
    _stream:     Option<Stream>,

    // Mouse (single-pointer) state.
    last_mouse:      Option<Pos2>,
    last_mouse_time: f64,
    press_pos:       Option<Pos2>,
    is_dragging:     bool,

    // Multi-touch: one entry per active finger.
    touches: HashMap<u64, TouchPoint>,

    // Controls.
    steps_per_frame: usize,
    color_mode:      ColorMode,
    wave_mode:       WaveMode,
    drum_type:       DrumType,

    // FPS.
    fps_history:     [f32; 60],
    fps_idx:         usize,
    last_frame_time: f64,

    // Cached canvas rect (needed when processing touch events outside the panel closure).
    canvas_rect: Rect,
}

impl FluidDrumScene {
    pub fn new() -> Self {
        let audio_state = Arc::new(Mutex::new(AudioState::new()));
        let stream      = setup_audio(Arc::clone(&audio_state));

        if let Some(ref s) = stream {
            use cpal::traits::StreamTrait;
            let _ = s.play();
        }

        let mut membrane = Membrane::new();
        // Subtle initial excitation so the screen isn't completely blank.
        for _ in 0..4 {
            let x = fastrand::usize(20..(GRID_W - 20));
            let y = fastrand::usize(20..(GRID_H - 20));
            membrane.excite_area(x, y, 3, 0.4);
        }

        Self {
            membrane,
            texture: None,
            audio_state,
            _stream: stream,
            last_mouse: None,
            last_mouse_time: 0.0,
            press_pos: None,
            is_dragging: false,
            touches: HashMap::new(),
            steps_per_frame: 3,
            color_mode: ColorMode::Ocean,
            wave_mode: WaveMode::Standard,
            drum_type: DrumType::Kick,
            fps_history: [60.0; 60],
            fps_idx: 0,
            last_frame_time: 0.0,
            canvas_rect: Rect::EVERYTHING,
        }
    }

    // ── Coordinate helpers ────────────────────────────────────────────────────

    fn to_grid(&self, pos: Pos2) -> (usize, usize) {
        let r  = self.canvas_rect;
        let rx = (pos.x - r.min.x) / r.width();
        let ry = (pos.y - r.min.y) / r.height();
        let gx = (rx * GRID_W as f32).clamp(1.0, GRID_W as f32 - 2.0) as usize;
        let gy = (ry * GRID_H as f32).clamp(1.0, GRID_H as f32 - 2.0) as usize;
        (gx, gy)
    }

    fn freq_for_pos(&self, pos: Pos2) -> f32 {
        let r    = self.canvas_rect;
        let cx   = r.center().x;
        let cy   = r.center().y;
        let diag = (r.width().powi(2) + r.height().powi(2)).sqrt();
        let dist = ((pos.x - cx).powi(2) + (pos.y - cy).powi(2)).sqrt();
        200.0 - (dist / diag) * 120.0
    }

    // ── Hit & drag ────────────────────────────────────────────────────────────

    fn do_hit(&mut self, pos: Pos2) {
        let (gx, gy) = self.to_grid(pos);
        let strength = 10.0 + fastrand::f32() * 5.0;
        self.membrane.excite_area(gx, gy, 3, strength);

        let freq = self.freq_for_pos(pos);
        if let Ok(mut st) = self.audio_state.lock() {
            st.trigger_hit(freq, self.drum_type);
        }
    }

    fn do_drag(&mut self, pos: Pos2, speed: f32) {
        let (gx, gy) = self.to_grid(pos);
        let strength = (2.0 + speed * 0.05).min(4.0);
        self.membrane.excite_area(gx, gy, 2, strength);

        let rel_y = (pos.y - self.canvas_rect.min.y) / self.canvas_rect.height();
        let freq  = 80.0 + rel_y * 400.0;
        let amp   = (speed * 0.01).clamp(0.0, 1.0);
        if let Ok(mut st) = self.audio_state.lock() {
            st.set_drag(freq, amp);
        }
    }

    // ── Multi-touch event processing ──────────────────────────────────────────

    fn process_touches(&mut self, ctx: &egui::Context, dt: f32) {
        let events: Vec<egui::Event> = ctx.input(|i| i.events.clone());

        for event in &events {
            if let egui::Event::Touch { id, phase, pos, .. } = event {
                let key = id.0;
                match phase {
                    egui::TouchPhase::Start => {
                        // Only act if touch is inside canvas.
                        if self.canvas_rect.contains(*pos) {
                            self.touches.insert(key, TouchPoint {
                                last_pos:  *pos,
                                start_pos: *pos,
                                is_drag:   false,
                            });
                        }
                    }
                    egui::TouchPhase::Move => {
                        // Snapshot the values we need before releasing the borrow.
                        let snapshot = self.touches.get_mut(&key).map(|tp| {
                            let delta     = *pos - tp.last_pos;
                            let speed     = delta.length() / dt.max(0.001);
                            if (*pos - tp.start_pos).length() > 5.0 { tp.is_drag = true; }
                            let is_drag   = tp.is_drag;
                            let last      = tp.last_pos;
                            tp.last_pos   = *pos;
                            (delta, speed, is_drag, last)
                        });
                        if let Some((delta, speed, is_drag, last)) = snapshot {
                            if is_drag {
                                let steps = (delta.length() / 4.0).ceil() as usize;
                                for s in 0..steps.max(1) {
                                    let t   = s as f32 / steps.max(1) as f32;
                                    let mid = last + delta * t;
                                    self.do_drag(mid, speed);
                                }
                            } else {
                                let (gx, gy) = self.to_grid(*pos);
                                self.membrane.excite_area(gx, gy, 1, 1.5);
                            }
                        }
                    }
                    egui::TouchPhase::End | egui::TouchPhase::Cancel => {
                        if let Some(tp) = self.touches.remove(&key) {
                            if !tp.is_drag {
                                // Tap.
                                self.do_hit(tp.last_pos);
                            } else {
                                // Stop drag audio if no other fingers are still dragging.
                                let any_drag = self.touches.values().any(|t| t.is_drag);
                                if !any_drag {
                                    if let Ok(mut st) = self.audio_state.lock() {
                                        st.stop_drag();
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // ── FPS ───────────────────────────────────────────────────────────────────

    fn avg_fps(&self) -> f32 {
        self.fps_history.iter().sum::<f32>() / self.fps_history.len() as f32
    }

    pub fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.request_repaint();

        let now = ctx.input(|i| i.time);
        let dt  = ((now - self.last_frame_time) as f32).clamp(0.001, 0.1);
        self.fps_history[self.fps_idx] = 1.0 / dt;
        self.fps_idx = (self.fps_idx + 1) % self.fps_history.len();
        self.last_frame_time = now;

        // Process multi-touch events before rendering.
        self.process_touches(ctx, dt);

        // ── Side panel ────────────────────────────────────────────────────────
        egui::SidePanel::right("controls")
            .min_width(270.0)
            .max_width(320.0)
            .resizable(false)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.add_space(10.0);
                    ui.label(RichText::new("Fluid Drum")
                        .size(30.0).color(Color32::from_rgb(80, 200, 255)).strong());
                    ui.label(RichText::new("Tap to drum  •  Drag to sing  •  Multi-touch")
                        .italics().color(Color32::from_rgb(130, 130, 160)).size(12.0));

                    ui.add_space(10.0);
                    ui.separator();
                    ui.add_space(6.0);

                    // ── Drum type ─────────────────────────────────────────────
                    ui.label(RichText::new("Drum").strong());
                    ui.horizontal_wrapped(|ui| {
                        for &dt in DrumType::ALL {
                            let sel = self.drum_type == dt;
                            if ui.selectable_label(sel, dt.label()).clicked() {
                                self.drum_type = dt;
                            }
                        }
                    });

                    ui.add_space(6.0);
                    ui.separator();
                    ui.add_space(6.0);

                    // ── Wave mode ─────────────────────────────────────────────
                    ui.label(RichText::new("Wave").strong());
                    ui.horizontal_wrapped(|ui| {
                        for wm in WaveMode::ALL {
                            let sel = &self.wave_mode == wm;
                            if ui.selectable_label(sel, wm.label()).clicked() {
                                self.wave_mode = wm.clone();
                            }
                        }
                    });

                    ui.add_space(6.0);
                    ui.separator();
                    ui.add_space(6.0);

                    // ── Color mode ────────────────────────────────────────────
                    ui.label(RichText::new("Color").strong());
                    ui.horizontal_wrapped(|ui| {
                        for cm in ColorMode::ALL {
                            let sel = &self.color_mode == cm;
                            if ui.selectable_label(sel, cm.label()).clicked() {
                                self.color_mode = cm.clone();
                            }
                        }
                    });

                    ui.add_space(6.0);
                    ui.separator();
                    ui.add_space(6.0);

                    // ── Physics sliders ───────────────────────────────────────
                    ui.label(RichText::new("Physics").strong());
                    ui.label("Steps / frame");
                    ui.add(egui::Slider::new(&mut self.steps_per_frame, 1..=6));
                    ui.label("Damping (ring)");
                    ui.add(egui::Slider::new(&mut self.membrane.damping, 0.990..=0.9995)
                        .fixed_decimals(4));
                    ui.label("Wave speed");
                    ui.add(egui::Slider::new(&mut self.membrane.wave_speed, 0.30..=0.48)
                        .fixed_decimals(2));

                    ui.add_space(8.0);
                    if ui.add(egui::Button::new(
                            RichText::new("  Clear  ").color(Color32::from_rgb(255, 80, 80)))
                        .min_size(Vec2::new(90.0, 28.0)))
                        .clicked()
                    {
                        self.membrane.clear();
                    }

                    ui.add_space(10.0);
                    ui.separator();
                    ui.add_space(4.0);
                    ui.label(RichText::new(format!("{:.0} fps  •  {}×{} grid",
                        self.avg_fps(), GRID_W, GRID_H))
                        .color(Color32::from_gray(110)).size(11.0));

                    ui.add_space(10.0);
                    ui.separator();
                    ui.add_space(4.0);

                    // Physics explanation (collapsible)
                    egui::CollapsingHeader::new(
                        RichText::new("How it works")
                            .size(12.0)
                            .color(Color32::from_rgb(100, 200, 200)),
                    )
                    .default_open(false)
                    .show(ui, |ui| {
                        let dim = Color32::from_rgba_unmultiplied(150, 200, 195, 220);
                        let hi  = Color32::from_rgb(80, 220, 200);

                        ui.add_space(4.0);
                        ui.label(RichText::new("2D Wave Equation").size(11.0).color(hi).strong());
                        ui.label(RichText::new(
                            "The membrane solves the wave PDE on a \
                             200×160 grid each frame:\n\
                             ∂²u/∂t² = c²·∇²u\n\n\
                             ∇²u is the discrete Laplacian of \
                             displacement.  c is wave speed; \
                             stability requires c ≤ 1/√2 ≈ 0.71."
                        ).size(10.0).color(dim));

                        ui.add_space(6.0);
                        ui.label(RichText::new("Wave Modes").size(11.0).color(hi).strong());
                        ui.label(RichText::new(
                            "Membrane — fixed (reflective) boundaries.\n\
                             Ripple — absorbing edges: waves fade out \
                             instead of reflecting, like an open ocean.\n\
                             Interference — 9-point stencil: diagonal \
                             neighbours get half weight, making speed \
                             anisotropic and creating moiré patterns.\n\
                             Vortex — after each step a clockwise \
                             rotational bias nudges energy spiralling inward."
                        ).size(10.0).color(dim));

                        ui.add_space(6.0);
                        ui.label(RichText::new("Sound").size(11.0).color(hi).strong());
                        ui.label(RichText::new(
                            "Tap = percussive drum hit synthesized \
                             with exponential decay and harmonics.  \
                             Drag = continuous tonal excitation whose \
                             pitch maps to vertical position.  \
                             Multi-touch: each finger is independent."
                        ).size(10.0).color(dim));
                    });
                });
            });

        // ── Central panel: canvas ─────────────────────────────────────────────
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(Color32::from_rgb(5, 5, 16)))
            .show(ctx, |ui| {
                let rect = ui.available_rect_before_wrap();
                self.canvas_rect = rect;

                // Simulate.
                for _ in 0..self.steps_per_frame {
                    self.membrane.step(&self.wave_mode);
                }

                // Render.
                let img = self.membrane.render_to_image(
                    rect.width() as usize, rect.height() as usize, &self.color_mode,
                );
                if let Some(ref mut tex) = self.texture {
                    tex.set(img, TextureOptions::NEAREST);
                } else {
                    self.texture = Some(ui.ctx().load_texture(
                        "membrane", img, TextureOptions::NEAREST,
                    ));
                }
                if let Some(ref tex) = self.texture {
                    ui.painter().image(
                        tex.id(), rect,
                        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                        Color32::WHITE,
                    );
                }

                // Draw touch-point indicators.
                for tp in self.touches.values() {
                    let col = if tp.is_drag {
                        Color32::from_rgba_unmultiplied(80, 220, 255, 100)
                    } else {
                        Color32::from_rgba_unmultiplied(255, 200, 80, 120)
                    };
                    ui.painter().circle_stroke(tp.last_pos, 24.0, egui::Stroke::new(2.0, col));
                }

                // ── Mouse interaction ─────────────────────────────────────────
                let response = ui.allocate_rect(rect, egui::Sense::click_and_drag());

                if response.drag_started() {
                    if let Some(pos) = ctx.input(|i| i.pointer.interact_pos()) {
                        self.press_pos      = Some(pos);
                        self.last_mouse     = Some(pos);
                        self.last_mouse_time = now;
                        self.is_dragging    = false;
                    }
                }

                if response.dragged() {
                    if let Some(pos) = ctx.input(|i| i.pointer.interact_pos()) {
                        if let Some(last) = self.last_mouse {
                            let delta = pos - last;
                            let speed = delta.length() / dt;
                            if let Some(press) = self.press_pos {
                                if (pos - press).length() > 5.0 { self.is_dragging = true; }
                            }
                            if self.is_dragging {
                                self.do_drag(pos, speed);
                                let steps = (delta.length() / 4.0).ceil() as usize;
                                for s in 1..steps {
                                    let t   = s as f32 / steps as f32;
                                    let mid = last + delta * t;
                                    let (gx, gy) = self.to_grid(mid);
                                    self.membrane.excite_area(gx, gy, 1, 1.5);
                                }
                            }
                        }
                        self.last_mouse     = Some(pos);
                        self.last_mouse_time = now;
                    }
                }

                if response.drag_stopped() {
                    if let Some(pos) = ctx.input(|i| i.pointer.interact_pos()) {
                        if !self.is_dragging { self.do_hit(pos); }
                        if let Ok(mut st) = self.audio_state.lock() { st.stop_drag(); }
                    }
                    self.press_pos   = None;
                    self.last_mouse  = None;
                    self.is_dragging = false;
                }

                if response.clicked() && !self.is_dragging {
                    if let Some(pos) = ctx.input(|i| i.pointer.interact_pos()) {
                        self.do_hit(pos);
                    }
                }

                if response.hovered() {
                    ctx.set_cursor_icon(egui::CursorIcon::Crosshair);
                }

                // Draw mouse cursor ring.
                if let Some(pos) = ctx.input(|i| i.pointer.latest_pos()) {
                    if rect.contains(pos) {
                        ui.painter().circle_stroke(
                            pos, 18.0,
                            egui::Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 60)),
                        );
                    }
                }
            });
    }
}
