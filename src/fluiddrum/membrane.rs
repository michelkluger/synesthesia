#![allow(dead_code, unused_imports, unused_variables)]

pub const GRID_W: usize = 200;
pub const GRID_H: usize = 160;

// ─── Wave mode ────────────────────────────────────────────────────────────────

/// Physics behaviour of the simulated membrane.
#[derive(Debug, Clone, PartialEq)]
pub enum WaveMode {
    /// Standard 2D wave equation, fixed (reflective) boundaries.
    Standard,
    /// Absorbing boundary: waves die smoothly at the edges — no reflection,
    /// clean infinite-ocean feel.
    Ripple,
    /// 9-point stencil: diagonal neighbours get half-weight, making wave speed
    /// anisotropic and producing subtle moiré / interference patterns.
    Interference,
    /// After each step a small rotational bias is added, gradually spiralling
    /// energy toward one side and producing vortex-like swept patterns.
    Vortex,
}

impl WaveMode {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Standard     => "Membrane",
            Self::Ripple       => "Ripple",
            Self::Interference => "Interference",
            Self::Vortex       => "Vortex",
        }
    }

    pub const ALL: &'static [WaveMode] =
        &[WaveMode::Standard, WaveMode::Ripple, WaveMode::Interference, WaveMode::Vortex];
}

// ─── Color mode ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum ColorMode {
    // Original four
    Ocean, Fire, Plasma, Mono,
    // New
    Lava,    // black → deep red → orange → white
    Aurora,  // dark → green → teal → white (crests), dark → purple (troughs)
    Neon,    // hue-mapped by displacement, always bright
    Oil,     // iridescent rainbow: hue from displacement, value from |d|
}

impl ColorMode {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Ocean  => "Ocean",
            Self::Fire   => "Fire",
            Self::Plasma => "Plasma",
            Self::Mono   => "Mono",
            Self::Lava   => "Lava",
            Self::Aurora => "Aurora",
            Self::Neon   => "Neon",
            Self::Oil    => "Oil",
        }
    }

    pub const ALL: &'static [ColorMode] = &[
        ColorMode::Ocean, ColorMode::Fire,  ColorMode::Plasma, ColorMode::Mono,
        ColorMode::Lava,  ColorMode::Aurora, ColorMode::Neon,  ColorMode::Oil,
    ];
}

// ─── Membrane ─────────────────────────────────────────────────────────────────

pub struct Membrane {
    pub cur:        Vec<f32>,
    pub prev:       Vec<f32>,
    pub wave_speed: f32,
    pub damping:    f32,
    /// Pre-baked edge-absorption mask for Ripple mode (0 at edges → 1 interior).
    edge_mask:      Vec<f32>,
    /// Vortex frame counter — drives a slow angular bias.
    vortex_tick:    u32,
}

impl Membrane {
    pub fn new() -> Self {
        let size = GRID_W * GRID_H;
        let edge_mask = build_edge_mask();
        Self {
            cur:        vec![0.0; size],
            prev:       vec![0.0; size],
            wave_speed: 0.45,
            damping:    0.998,
            edge_mask,
            vortex_tick: 0,
        }
    }

    pub fn clear(&mut self) {
        self.cur.fill(0.0);
        self.prev.fill(0.0);
    }

    pub fn excite(&mut self, x: usize, y: usize, strength: f32) {
        if x > 0 && x < GRID_W - 1 && y > 0 && y < GRID_H - 1 {
            let idx = y * GRID_W + x;
            self.cur[idx] = (self.cur[idx] + strength).clamp(-2.0, 2.0);
        }
    }

    pub fn excite_area(&mut self, x: usize, y: usize, radius: usize, strength: f32) {
        let r = radius as isize;
        for dy in -r..=r {
            for dx in -r..=r {
                if dx * dx + dy * dy > r * r { continue; }
                let nx = x as isize + dx;
                let ny = y as isize + dy;
                if nx <= 0 || nx >= GRID_W as isize - 1 || ny <= 0 || ny >= GRID_H as isize - 1 {
                    continue;
                }
                let dist  = ((dx * dx + dy * dy) as f32).sqrt();
                let fall  = 1.0 - dist / (radius as f32 + 1.0);
                let idx   = ny as usize * GRID_W + nx as usize;
                self.cur[idx] = (self.cur[idx] + strength * fall).clamp(-2.0, 2.0);
            }
        }
    }

    pub fn step(&mut self, mode: &WaveMode) {
        let c2  = self.wave_speed * self.wave_speed;
        let dmp = self.damping;
        let w   = GRID_W;
        let h   = GRID_H;

        match mode {
            WaveMode::Standard => {
                let mut next = vec![0.0f32; w * h];
                for y in 1..(h - 1) {
                    for x in 1..(w - 1) {
                        let i   = y * w + x;
                        let lap = self.cur[i-1] + self.cur[i+1]
                                + self.cur[i-w] + self.cur[i+w]
                                - 4.0 * self.cur[i];
                        next[i] = (2.0 * self.cur[i] - self.prev[i] + c2 * lap)
                            .clamp(-2.0, 2.0) * dmp;
                    }
                }
                std::mem::swap(&mut self.prev, &mut self.cur);
                self.cur = next;
            }

            WaveMode::Ripple => {
                // Standard wave equation, then multiply by absorbing edge mask.
                let mut next = vec![0.0f32; w * h];
                for y in 1..(h - 1) {
                    for x in 1..(w - 1) {
                        let i   = y * w + x;
                        let lap = self.cur[i-1] + self.cur[i+1]
                                + self.cur[i-w] + self.cur[i+w]
                                - 4.0 * self.cur[i];
                        let val = (2.0 * self.cur[i] - self.prev[i] + c2 * lap)
                            .clamp(-2.0, 2.0) * dmp;
                        next[i] = val * self.edge_mask[i];
                    }
                }
                std::mem::swap(&mut self.prev, &mut self.cur);
                self.cur = next;
            }

            WaveMode::Interference => {
                // 9-point stencil: cardinal at weight 0.5, diagonal at weight 0.25.
                // Produces different wave speed in diagonal vs cardinal → interference.
                let mut next = vec![0.0f32; w * h];
                for y in 1..(h - 1) {
                    for x in 1..(w - 1) {
                        let i = y * w + x;
                        let cardinal = self.cur[i-1] + self.cur[i+1]
                                     + self.cur[i-w] + self.cur[i+w];
                        let diagonal = self.cur[i-w-1] + self.cur[i-w+1]
                                     + self.cur[i+w-1] + self.cur[i+w+1];
                        let lap = cardinal * 0.5 + diagonal * 0.25
                                - self.cur[i] * (0.5*4.0 + 0.25*4.0);
                        next[i] = (2.0 * self.cur[i] - self.prev[i] + c2 * lap)
                            .clamp(-2.0, 2.0) * dmp;
                    }
                }
                std::mem::swap(&mut self.prev, &mut self.cur);
                self.cur = next;
            }

            WaveMode::Vortex => {
                // Standard step, then apply a small clockwise rotation bias:
                // each cell borrows a fraction from its clockwise neighbour.
                let mut next = vec![0.0f32; w * h];
                for y in 1..(h - 1) {
                    for x in 1..(w - 1) {
                        let i   = y * w + x;
                        let lap = self.cur[i-1] + self.cur[i+1]
                                + self.cur[i-w] + self.cur[i+w]
                                - 4.0 * self.cur[i];
                        next[i] = (2.0 * self.cur[i] - self.prev[i] + c2 * lap)
                            .clamp(-2.0, 2.0) * dmp;
                    }
                }
                // Rotational mixing: blend each cell 97% self + 3% clockwise neighbour.
                // Clockwise neighbour at (x,y): if above centre → right, right → below, etc.
                // Simplified: use a fixed angular offset that drifts.
                let bias = 0.025f32;
                let cx = (w / 2) as isize;
                let cy = (h / 2) as isize;
                let mut rotated = next.clone();
                for y in 1..(h - 1) {
                    for x in 1..(w - 1) {
                        let rx = x as isize - cx;
                        let ry = y as isize - cy;
                        // Tangential direction (clockwise): (-ry, rx) normalised.
                        let len = ((rx*rx + ry*ry) as f32).sqrt().max(1.0);
                        let tx = (-ry as f32 / len).round() as isize;
                        let ty = (rx as f32 / len).round() as isize;
                        let nx2 = (x as isize + tx).clamp(1, w as isize - 2) as usize;
                        let ny2 = (y as isize + ty).clamp(1, h as isize - 2) as usize;
                        let i  = y * w + x;
                        let ni = ny2 * w + nx2;
                        rotated[i] = next[i] * (1.0 - bias) + next[ni] * bias;
                    }
                }
                self.vortex_tick = self.vortex_tick.wrapping_add(1);
                std::mem::swap(&mut self.prev, &mut self.cur);
                self.cur = rotated;
            }
        }
    }

    /// Build a `ColorImage` scaled to (out_w × out_h) from the current grid.
    pub fn render_to_image(
        &self, out_w: usize, out_h: usize, color_mode: &ColorMode,
    ) -> egui::ColorImage {
        let mut pixels = Vec::with_capacity(out_w * out_h);
        for py in 0..out_h {
            let gy = (py * GRID_H / out_h).min(GRID_H - 1);
            for px in 0..out_w {
                let gx = (px * GRID_W / out_w).min(GRID_W - 1);
                let d  = self.cur[gy * GRID_W + gx];
                pixels.push(displacement_to_color(d, color_mode));
            }
        }
        egui::ColorImage { size: [out_w, out_h], pixels }
    }
}

// ─── Edge absorption mask ─────────────────────────────────────────────────────

/// Pre-bake a smooth window that is 0 at the boundary and 1 in the interior.
/// A 20-cell fade-in on each side gives clean absorption in Ripple mode.
fn build_edge_mask() -> Vec<f32> {
    const FADE: f32 = 20.0;
    (0..GRID_W * GRID_H).map(|i| {
        let x = (i % GRID_W) as f32;
        let y = (i / GRID_W) as f32;
        let mx = ((x / FADE).min(1.0) * ((GRID_W as f32 - 1.0 - x) / FADE).min(1.0)).sqrt();
        let my = ((y / FADE).min(1.0) * ((GRID_H as f32 - 1.0 - y) / FADE).min(1.0)).sqrt();
        mx * my
    }).collect()
}

// ─── Color mapping ────────────────────────────────────────────────────────────

fn displacement_to_color(d: f32, mode: &ColorMode) -> egui::Color32 {
    let d = d.clamp(-1.0, 1.0);

    match mode {
        ColorMode::Ocean => {
            if d > 0.0 {
                let t = d.sqrt();
                let r = t * t;
                let g = t * 0.85;
                let b = 0.06 + t * 0.94;
                rgb(r, g, b)
            } else {
                let t = (-d).sqrt();
                rgb(0.02, 0.02, 0.06 + t * 0.80)
            }
        }

        ColorMode::Fire => {
            let t = d.abs().sqrt();
            rgb(t, (t * 2.0 - 1.0).max(0.0), (t * 3.0 - 2.5).max(0.0))
        }

        ColorMode::Plasma => {
            let r = d.max(0.0) * 0.78 + (-d).max(0.0) * 0.47;
            let g = d.max(0.0) * 0.86;
            let b = d.abs();
            rgb(r, g, b)
        }

        ColorMode::Mono => {
            let v = d.abs().sqrt();
            rgb(v, v, v)
        }

        ColorMode::Lava => {
            // black → deep red → orange → yellow-white
            let t = d.abs().sqrt();
            let r = t;
            let g = (t * 2.5 - 1.2).clamp(0.0, 1.0);
            let b = (t * 5.0 - 4.0).clamp(0.0, 0.6);
            rgb(r, g, b)
        }

        ColorMode::Aurora => {
            if d > 0.0 {
                // dark → green → teal → white
                let t = d.sqrt();
                rgb(t * t * 0.6, t * 0.9, t * 0.55 + t * t * 0.45)
            } else {
                // dark → deep purple
                let t = (-d).sqrt();
                rgb(t * 0.5, 0.0, t * 0.85)
            }
        }

        ColorMode::Neon => {
            // Hue cycles through full spectrum, brightness by |d|.
            let hue   = (d + 1.0) * 0.5;            // 0→1 across [-1, 1]
            let value = d.abs().powf(0.5).max(0.05); // stay slightly bright even at rest
            hsv_to_rgb(hue, 1.0, value)
        }

        ColorMode::Oil => {
            // Iridescent: hue from displacement, chroma from gradient magnitude.
            // Use a compressed hue range for a more oil-like look.
            let hue   = (d * 2.5 + 0.55).rem_euclid(1.0);
            let value = d.abs().powf(0.35).max(0.0);
            hsv_to_rgb(hue, 0.85, value)
        }
    }
}

// ─── Color helpers ────────────────────────────────────────────────────────────

#[inline]
fn rgb(r: f32, g: f32, b: f32) -> egui::Color32 {
    egui::Color32::from_rgb(
        (r.clamp(0.0, 1.0) * 255.0) as u8,
        (g.clamp(0.0, 1.0) * 255.0) as u8,
        (b.clamp(0.0, 1.0) * 255.0) as u8,
    )
}

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> egui::Color32 {
    let h6     = h * 6.0;
    let sector = h6.floor() as i32;
    let frac   = h6 - h6.floor();
    let p = v * (1.0 - s);
    let q = v * (1.0 - frac * s);
    let t = v * (1.0 - (1.0 - frac) * s);
    let (r, g, b) = match sector % 6 {
        0 => (v, t, p), 1 => (q, v, p), 2 => (p, v, t),
        3 => (p, q, v), 4 => (t, p, v), _ => (v, p, q),
    };
    rgb(r, g, b)
}
