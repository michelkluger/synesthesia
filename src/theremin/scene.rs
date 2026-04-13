#![allow(dead_code, unused_imports, unused_variables, clippy::cast_precision_loss)]

use egui::{
    epaint::{Mesh, Vertex},
    Color32, Painter, Pos2, Rect, Rgba, Sense, Stroke, Vec2,
};
use std::sync::{Arc, Mutex};

use super::audio::{AudioEngine, AudioState, Waveform};
use super::tutorial::{TutorialMode, TutorialSong, TutorialScale, SONGS, SCALES, ADVANCE_SECS, TOLERANCE_PX};

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

    tutorial: Option<TutorialMode>,
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
            tutorial: None,
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
                egui::ScrollArea::vertical().show(ui, |ui| {
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
                ui.add_space(6.0);

                // ── Tutorial / Learn ──────────────────────────────────────────
                ui.label(egui::RichText::new("Tutorial").size(13.0).color(sec));
                ui.add_space(4.0);

                // Song buttons
                ui.label(egui::RichText::new("Songs").size(11.0)
                    .color(Color32::from_rgb(140, 145, 165)));
                let mut new_tut: Option<TutorialMode> = None;
                for (i, song) in SONGS.iter().enumerate() {
                    let is_song     = matches!(&self.tutorial, Some(TutorialMode::Song     { song_idx, .. }) if *song_idx == i);
                    let is_autoplay = matches!(&self.tutorial, Some(TutorialMode::Autoplay { song_idx, .. }) if *song_idx == i);

                    ui.horizontal(|ui| {
                        // ── Practice label ───────────────────────────────────
                        let label = if is_song {
                            let prog = if let Some(TutorialMode::Song { note_idx, .. }) = &self.tutorial {
                                format!("{} [{}/{}]", song.name, note_idx, song.notes.len())
                            } else { song.name.to_string() };
                            egui::RichText::new(prog).size(12.0).color(Color32::from_rgb(255, 220, 80))
                        } else {
                            egui::RichText::new(song.name).size(12.0).color(Color32::from_rgb(200, 200, 220))
                        };
                        if ui.selectable_label(is_song, label)
                            .on_hover_text(song.description)
                            .clicked()
                        {
                            new_tut = Some(TutorialMode::Song { song_idx: i, note_idx: 0, time_on: 0.0 });
                        }

                        // ── Watch (autoplay) button ───────────────────────────
                        let watch_col = if is_autoplay {
                            Color32::from_rgb(255, 200, 50)
                        } else {
                            Color32::from_rgb(130, 120, 90)
                        };
                        if ui.selectable_label(
                            is_autoplay,
                            egui::RichText::new("▶").size(12.0).color(watch_col),
                        )
                        .on_hover_text("Watch computer play")
                        .clicked()
                        {
                            new_tut = Some(TutorialMode::Autoplay {
                                song_idx:     i,
                                note_idx:     0,
                                time_on_note: 0.0,
                                cursor_x:     0.0,
                                cursor_y:     0.0,
                            });
                        }
                    });
                }

                ui.add_space(6.0);

                // Scale buttons
                ui.label(egui::RichText::new("Scales").size(11.0)
                    .color(Color32::from_rgb(140, 145, 165)));
                for (i, scale) in SCALES.iter().enumerate() {
                    let active = matches!(&self.tutorial, Some(TutorialMode::Scale(idx)) if *idx == i);
                    let label = egui::RichText::new(scale.name).size(12.0).color(
                        if active { Color32::from_rgb(100, 230, 180) }
                        else { Color32::from_rgb(200, 200, 220) }
                    );
                    if ui.selectable_label(active, label)
                        .on_hover_text(scale.description)
                        .clicked()
                    {
                        new_tut = Some(TutorialMode::Scale(i));
                    }
                }

                // Apply new tutorial selection
                if let Some(t) = new_tut {
                    // Toggle off if clicking the already-active one
                    let same = match (&self.tutorial, &t) {
                        (Some(TutorialMode::Song     { song_idx: a, .. }), TutorialMode::Song     { song_idx: b, .. }) => a == b,
                        (Some(TutorialMode::Autoplay { song_idx: a, .. }), TutorialMode::Autoplay { song_idx: b, .. }) => a == b,
                        (Some(TutorialMode::Scale(a)), TutorialMode::Scale(b)) => a == b,
                        _ => false,
                    };
                    self.tutorial = if same { None } else { Some(t) };
                }

                // Stop button
                if self.tutorial.is_some() {
                    ui.add_space(4.0);
                    if ui.add(egui::Button::new(
                        egui::RichText::new("Stop Tutorial").size(12.0)
                            .color(Color32::from_rgb(200, 100, 100))
                    ).fill(Color32::from_rgba_unmultiplied(60, 20, 20, 180))).clicked() {
                        self.tutorial = None;
                    }
                }

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

                }); // end ScrollArea
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

            // Don't let mouse override audio while autoplay is running.
            let is_autoplay = matches!(&self.tutorial, Some(TutorialMode::Autoplay { .. }));

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

                // Push trail point (skip during autoplay — virtual cursor has its own trail)
                if !is_autoplay {
                    self.trail.push(TrailPoint {
                        pos,
                        freq,
                        vol: vol_raw,
                        age: 0.0,
                    });
                }

                // Update audio (only when autoplay is not driving it)
                if !is_autoplay {
                    if let Ok(mut s) = self.audio.state.try_lock() {
                        s.target_freq = freq;
                        s.target_vol = vol;
                        s.active = true;
                        s.waveform = self.waveform;
                    }
                }
            } else if !is_autoplay {
                // Silence only when autoplay is not running
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

            // Tutorial: tick note advancement, draw overlay (under the trail)
            self.tick_tutorial(mouse_pos, rect, dt);
            if let Some(ref tut) = self.tutorial {
                render_tutorial_overlay(tut, &painter, rect, self.pulse_t, mouse_pos);
            }

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

    // ── Tutorial helpers ──────────────────────────────────────────────────────

    fn tick_tutorial(&mut self, mouse_pos: Option<Pos2>, rect: Rect, dt: f32) {
        // ── Song mode: user plays ───────────────────────────────────────────────
        if let Some(TutorialMode::Song { song_idx, note_idx, time_on }) = &mut self.tutorial {
            let song = &SONGS[*song_idx];
            if *note_idx < song.notes.len() {
                let target_x = freq_to_canvas_x(song.notes[*note_idx].freq, rect);
                match mouse_pos {
                    Some(pos) if rect.contains(pos) && (pos.x - target_x).abs() < TOLERANCE_PX => {
                        *time_on += dt;
                        if *time_on >= ADVANCE_SECS {
                            *note_idx += 1;
                            *time_on = 0.0;
                        }
                    }
                    _ => { *time_on = (*time_on - dt * 4.0).max(0.0); }
                }
            }
            return;
        }

        // ── Autoplay mode: computer plays ───────────────────────────────────────
        // Phase 1: update state; collect audio command (avoids double-borrow of self).
        enum Cmd { Play(f32), Silence, Noop }
        let cmd = if let Some(TutorialMode::Autoplay {
            song_idx, note_idx, time_on_note, cursor_x, cursor_y,
        }) = &mut self.tutorial {
            let song = &SONGS[*song_idx];
            if *note_idx >= song.notes.len() {
                Cmd::Silence
            } else {
                let note      = &song.notes[*note_idx];
                let beat_dur  = 60.0 / song.bpm * note.beats;
                let note_freq = note.freq;
                let tx        = freq_to_canvas_x(note_freq, rect);

                // Smooth cursor lerp toward the target note X
                *cursor_x += (tx - *cursor_x) * (dt * 10.0).min(1.0);
                *cursor_y  = rect.center().y;

                // Add a trail point at the virtual cursor position
                // (handled after this block via a separate push)
                *time_on_note += dt;
                if *time_on_note >= beat_dur {
                    *time_on_note -= beat_dur;
                    *note_idx += 1;
                }

                Cmd::Play(note_freq)
            }
        } else {
            Cmd::Noop
        };

        // Phase 2: apply audio (self.tutorial borrow released above).
        match cmd {
            Cmd::Play(freq) => {
                let vol = 0.72 * self.master_volume;
                if let Ok(mut s) = self.audio.state.try_lock() {
                    s.target_freq = freq;
                    s.target_vol  = vol;
                    s.active      = true;
                    s.waveform    = self.waveform;
                }
                // Push a trail point at the virtual cursor so it leaves a glow
                if let Some(TutorialMode::Autoplay { cursor_x, cursor_y, .. }) = &self.tutorial {
                    self.trail.push(TrailPoint {
                        pos:  Pos2::new(*cursor_x, *cursor_y),
                        freq,
                        vol:  0.8,
                        age:  0.0,
                    });
                }
            }
            Cmd::Silence => {
                if let Ok(mut s) = self.audio.state.try_lock() {
                    s.active     = false;
                    s.target_vol = 0.0;
                }
            }
            Cmd::Noop => {}
        }
    }
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

/// Convert a frequency to a canvas X coordinate using the theremin's log mapping.
fn freq_to_canvas_x(freq: f32, rect: Rect) -> f32 {
    let ratio = (MAX_FREQ / MIN_FREQ).log2();
    let rel   = ((freq / MIN_FREQ).log2() / ratio).clamp(0.0, 1.0);
    rect.left() + rel * rect.width()
}

fn render_tutorial_overlay(
    mode: &TutorialMode, painter: &Painter, rect: Rect, pulse_t: f32, mouse_pos: Option<Pos2>,
) {
    match mode {
        TutorialMode::Song { song_idx, note_idx, time_on } =>
            render_song_overlay(painter, rect, &SONGS[*song_idx], *note_idx, *time_on, pulse_t),
        TutorialMode::Autoplay { song_idx, note_idx, cursor_x, cursor_y, .. } =>
            render_autoplay_overlay(painter, rect, &SONGS[*song_idx], *note_idx, *cursor_x, *cursor_y, pulse_t),
        TutorialMode::Scale(idx) =>
            render_scale_overlay(painter, rect, &SCALES[*idx], mouse_pos),
    }
}

fn render_song_overlay(
    painter: &Painter, rect: Rect, song: &TutorialSong,
    note_idx: usize, time_on: f32, pulse_t: f32,
) {
    let cy    = rect.center().y;
    let total = song.notes.len();

    // Faint centre guide line
    painter.line_segment(
        [Pos2::new(rect.left(), cy), Pos2::new(rect.right(), cy)],
        Stroke::new(1.0, Color32::from_rgba_unmultiplied(100, 100, 130, 30)),
    );

    for (i, note) in song.notes.iter().enumerate() {
        let x   = freq_to_canvas_x(note.freq, rect);
        let pos = Pos2::new(x, cy);
        let hue = freq_to_hue(note.freq);

        if i < note_idx {
            // Already played — small green tick
            painter.circle_filled(pos, 4.0, Color32::from_rgba_unmultiplied(80, 200, 100, 100));
        } else if i == note_idx && note_idx < total {
            // Current target
            let pulse    = (pulse_t.sin() * 0.5 + 0.5) * 0.4 + 0.6;
            let progress = (time_on / ADVANCE_SECS).clamp(0.0, 1.0);
            let col      = hue_to_color(hue, 1.0, 1.0, 230);

            // Tolerance zone (faint band)
            painter.rect_filled(
                egui::Rect::from_center_size(Pos2::new(x, rect.center().y),
                    Vec2::new(TOLERANCE_PX * 2.0, rect.height())),
                0.0, Color32::from_rgba_unmultiplied(200, 200, 255, 7),
            );
            // Vertical guide
            painter.line_segment(
                [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
                Stroke::new(1.0, Color32::from_rgba_unmultiplied(180, 180, 255, 25)),
            );
            // Progress fill circle
            painter.circle_filled(pos, 26.0, Color32::from_rgba_unmultiplied(40, 40, 70, 120));
            if progress > 0.0 {
                let g = (200.0 * progress) as u8;
                painter.circle_filled(pos, 26.0 * progress,
                    Color32::from_rgba_unmultiplied(60, g, 80, (progress * 160.0) as u8));
            }
            // Outer pulsing ring
            painter.circle_stroke(pos, 22.0 * pulse, Stroke::new(2.5, col));
            // Note label
            painter.text(Pos2::new(x, cy - 40.0), egui::Align2::CENTER_BOTTOM,
                note.label, egui::FontId::proportional(16.0), col);
            // "Hold!" cue
            if time_on > 0.06 && progress < 1.0 {
                painter.text(Pos2::new(x, cy + 40.0), egui::Align2::CENTER_TOP,
                    "Hold!", egui::FontId::proportional(12.0),
                    Color32::from_rgba_unmultiplied(210, 240, 150, 210));
            }

        } else if i == note_idx + 1 {
            // Next note hint
            let col = hue_to_color(hue, 0.7, 0.8, 90);
            painter.circle_stroke(pos, 11.0, Stroke::new(1.5, col));
            painter.text(Pos2::new(x, cy - 20.0), egui::Align2::CENTER_BOTTOM,
                note.label, egui::FontId::proportional(11.0),
                Color32::from_rgba_unmultiplied(160, 160, 200, 110));
        } else {
            // Future — tiny dim dot
            painter.circle_filled(pos, 3.0, hue_to_color(hue, 0.5, 0.6, 45));
        }
    }

    // Progress counter (top-left)
    let text = if note_idx >= total {
        format!("Complete!  {}  {}/{}", song.name, total, total)
    } else {
        format!("{}   {}/{}", song.name, note_idx, total)
    };
    painter.text(Pos2::new(rect.left() + 10.0, rect.top() + 10.0),
        egui::Align2::LEFT_TOP, &text, egui::FontId::proportional(12.0),
        Color32::from_rgba_unmultiplied(180, 180, 210, 180));

    if note_idx >= total {
        painter.text(rect.center(), egui::Align2::CENTER_CENTER,
            "All notes played!",
            egui::FontId::proportional(28.0),
            Color32::from_rgba_unmultiplied(180, 255, 150, 230));
    }
}

fn render_autoplay_overlay(
    painter: &Painter, rect: Rect, song: &TutorialSong,
    note_idx: usize, cursor_x: f32, cursor_y: f32, pulse_t: f32,
) {
    let cy    = rect.center().y;
    let total = song.notes.len();

    // Faint guide line
    painter.line_segment(
        [Pos2::new(rect.left(), cy), Pos2::new(rect.right(), cy)],
        Stroke::new(1.0, Color32::from_rgba_unmultiplied(100, 100, 130, 30)),
    );

    for (i, note) in song.notes.iter().enumerate() {
        let x   = freq_to_canvas_x(note.freq, rect);
        let pos = Pos2::new(x, cy);
        let hue = freq_to_hue(note.freq);

        if i < note_idx {
            painter.circle_filled(pos, 4.0, Color32::from_rgba_unmultiplied(80, 200, 100, 100));
        } else if i == note_idx && note_idx < total {
            // Destination target ring
            let col = hue_to_color(hue, 1.0, 1.0, 120);
            painter.rect_filled(
                egui::Rect::from_center_size(Pos2::new(x, rect.center().y),
                    Vec2::new(TOLERANCE_PX * 2.0, rect.height())),
                0.0, Color32::from_rgba_unmultiplied(200, 200, 255, 5),
            );
            painter.line_segment(
                [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
                Stroke::new(1.0, Color32::from_rgba_unmultiplied(180, 180, 255, 18)),
            );
            painter.text(Pos2::new(x, cy - 42.0), egui::Align2::CENTER_BOTTOM,
                note.label, egui::FontId::proportional(16.0),
                hue_to_color(hue, 0.7, 1.0, 200));
        } else if i == note_idx + 1 {
            let col = hue_to_color(hue, 0.7, 0.8, 90);
            painter.circle_stroke(pos, 11.0, Stroke::new(1.5, col));
            painter.text(Pos2::new(x, cy - 20.0), egui::Align2::CENTER_BOTTOM,
                note.label, egui::FontId::proportional(11.0),
                Color32::from_rgba_unmultiplied(160, 160, 200, 110));
        } else {
            painter.circle_filled(pos, 3.0, hue_to_color(hue, 0.5, 0.6, 45));
        }
    }

    // ── Golden autoplay cursor orb ────────────────────────────────────────────
    if note_idx < total {
        let pulse     = (pulse_t.sin() * 0.5 + 0.5) * 0.4 + 0.6;
        let orb       = Pos2::new(cursor_x, cursor_y);

        // Outer halo
        painter.circle_filled(orb, 36.0 * pulse,
            Color32::from_rgba_unmultiplied(255, 190, 30, 16));
        // Mid glow
        painter.circle_filled(orb, 22.0 * pulse,
            Color32::from_rgba_unmultiplied(255, 200, 60, 45));
        // Core
        painter.circle_filled(orb, 11.0,
            Color32::from_rgba_unmultiplied(255, 230, 90, 240));
        // Pulsing ring
        painter.circle_stroke(orb, 17.0 * pulse,
            Stroke::new(2.5, Color32::from_rgba_unmultiplied(255, 245, 160, 200)));
        // Inner bright spot
        painter.circle_filled(orb, 4.0,
            Color32::from_rgba_unmultiplied(255, 255, 220, 255));
    }

    // Status line
    let text = if note_idx >= total {
        format!("✓  {}  —  complete!", song.name)
    } else {
        format!("▶  {}   {}/{}", song.name, note_idx, total)
    };
    painter.text(Pos2::new(rect.left() + 10.0, rect.top() + 10.0),
        egui::Align2::LEFT_TOP, &text, egui::FontId::proportional(12.0),
        Color32::from_rgba_unmultiplied(255, 215, 80, 210));

    if note_idx >= total {
        painter.text(rect.center(), egui::Align2::CENTER_CENTER,
            "Performance complete!",
            egui::FontId::proportional(28.0),
            Color32::from_rgba_unmultiplied(255, 200, 80, 230));
    }
}

fn render_scale_overlay(
    painter: &Painter, rect: Rect, scale: &TutorialScale, mouse_pos: Option<Pos2>,
) {
    let cursor_x = mouse_pos
        .filter(|p| rect.contains(*p))
        .map(|p| p.x);

    for (i, &(freq, label)) in scale.notes.iter().enumerate() {
        let x       = freq_to_canvas_x(freq, rect);
        let is_root = i == 0 || i + 1 == scale.notes.len();
        let near    = cursor_x.map(|cx| (cx - x).abs() < TOLERANCE_PX * 1.5).unwrap_or(false);
        let hue     = freq_to_hue(freq);

        let alpha   = if near { 110u8 } else if is_root { 55 } else { 32 };
        let band_w  = if near { 38.0f32 } else { 22.0 };

        // Coloured band
        painter.rect_filled(
            egui::Rect::from_center_size(Pos2::new(x, rect.center().y),
                Vec2::new(band_w, rect.height())),
            0.0, hue_to_color(hue, 0.85, 0.9, alpha),
        );
        // Centre line
        painter.line_segment(
            [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
            Stroke::new(if near { 1.5 } else { 0.5 },
                hue_to_color(hue, 1.0, 1.0, if near { 190 } else { 55 })),
        );
        // Label
        let font_sz = if near { 14.0 } else if is_root { 13.0 } else { 11.0 };
        let l_alpha = if near { 255u8 } else if is_root { 200 } else { 130 };
        painter.text(Pos2::new(x, rect.top() + 18.0), egui::Align2::CENTER_CENTER,
            label, egui::FontId::proportional(font_sz),
            hue_to_color(hue, 0.5, 1.0, l_alpha));
    }

    painter.text(Pos2::new(rect.left() + 10.0, rect.top() + 10.0),
        egui::Align2::LEFT_TOP, scale.name, egui::FontId::proportional(12.0),
        Color32::from_rgba_unmultiplied(180, 180, 210, 160));
}

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
