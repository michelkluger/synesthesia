//! Physics: Chladni figure simulation.
//!
//! A square plate vibrating at mode (m, n) has displacement:
//!
//!   Z(x, y) = cos(m·π·x/W) · cos(n·π·y/H) − cos(n·π·x/W) · cos(m·π·y/H)
//!
//! Nodal lines are where Z = 0.  Particles (sand grains) are driven toward
//! the nodal lines by following the negative gradient of |Z|, plus a small
//! Brownian jitter to keep things lively.

use glam::Vec2;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

pub const PARTICLE_COUNT: usize = 3_000;

/// Gradient finite-difference step (pixels).
const EPS: f32 = 2.0;

/// How strongly particles are pushed toward nodal lines.
///
/// Tuning: terminal velocity ≈ grad × FORCE_STRENGTH × dt / (1 − DAMPING).
/// grad ≈ m·π / plate_width ≈ 0.008 for m=2, width=800.
/// With DAMPING=0.97 and dt=0.016 → terminal ≈ 0.008 × 15000 × 0.016 / 0.03 ≈ 64 px/s.
const FORCE_STRENGTH: f32 = 15_000.0;

/// Brownian jitter amplitude — keeps particles alive on nodal lines.
const JITTER: f32 = 25.0;

/// Velocity damping applied each sub-step (0 = instant stop, 1 = no friction).
const DAMPING: f32 = 0.97;

/// Number of physics sub-steps executed per visual frame.
pub const STEPS_PER_FRAME: usize = 4;

// ---------------------------------------------------------------------------
// Particle
// ---------------------------------------------------------------------------

pub struct Particle {
    pub pos: Vec2,
    pub vel: Vec2,
    /// Cached |Z| value at this position — used for colorising.
    pub displacement: f32,
}

impl Particle {
    pub fn new_random(width: f32, height: f32) -> Self {
        Self {
            pos: Vec2::new(
                fastrand::f32() * width,
                fastrand::f32() * height,
            ),
            vel: Vec2::ZERO,
            displacement: 1.0,
        }
    }

    /// Scatter to a fresh random position anywhere on the plate.
    pub fn scatter(&mut self, width: f32, height: f32) {
        self.pos = Vec2::new(fastrand::f32() * width, fastrand::f32() * height);
        self.vel = Vec2::ZERO;
    }
}

// ---------------------------------------------------------------------------
// Chladni field
// ---------------------------------------------------------------------------

/// Compute the Chladni displacement Z(x, y) for modes (m, n) on a plate
/// of size (width × height).  Coordinates are in *pixels* measured from the
/// top-left corner of the plate.
#[inline]
pub fn chladni_z(px: f32, py: f32, m: i32, n: i32, width: f32, height: f32) -> f32 {
    use std::f32::consts::PI;
    let mf = m as f32;
    let nf = n as f32;
    let x = px / width;   // normalised 0..1
    let y = py / height;

    (mf * PI * x).cos() * (nf * PI * y).cos()
        - (nf * PI * x).cos() * (mf * PI * y).cos()
}

/// Gradient of |Z| at (px, py) using central finite differences.
/// Returns a 2-D vector pointing in the direction of increasing |Z|.
#[inline]
fn grad_abs_z(px: f32, py: f32, m: i32, n: i32, width: f32, height: f32) -> Vec2 {
    let zx_p = chladni_z(px + EPS, py, m, n, width, height).abs();
    let zx_m = chladni_z(px - EPS, py, m, n, width, height).abs();
    let zy_p = chladni_z(px, py + EPS, m, n, width, height).abs();
    let zy_m = chladni_z(px, py - EPS, m, n, width, height).abs();
    Vec2::new((zx_p - zx_m) / (2.0 * EPS), (zy_p - zy_m) / (2.0 * EPS))
}

// ---------------------------------------------------------------------------
// Frequency → mode mapping
// ---------------------------------------------------------------------------

/// Map a frequency in Hz to Chladni mode indices (m, n).
///
/// Low frequencies → small (m, n) → simple cross/diamond patterns.
/// Higher frequencies → larger (m, n) → intricate lace-like patterns.
pub fn freq_to_mode(freq: f32) -> (i32, i32) {
    let base = (freq / 200.0).floor() as i32;
    let m = 1 + base.min(8);
    let n = 1 + ((((freq / 200.0 * 1.618).floor() as i32) % 5) + 5) % 5;
    (m, n)
}

// ---------------------------------------------------------------------------
// Simulation step
// ---------------------------------------------------------------------------

/// Advance all particles by one sub-step `dt` under the current Chladni field.
///
/// Call this `STEPS_PER_FRAME` times per visual frame for smooth integration.
pub fn update(
    particles: &mut Vec<Particle>,
    m: i32,
    n: i32,
    dt: f32,
    plate_w: f32,
    plate_h: f32,
) {
    for p in particles.iter_mut() {
        // --- Chladni force: gradient of |Z| points away from nodal lines;
        //     negate it to pull particles toward zero-crossings. ---
        let grad  = grad_abs_z(p.pos.x, p.pos.y, m, n, plate_w, plate_h);
        let force = -grad * FORCE_STRENGTH;

        // --- Brownian jitter keeps grains alive even when settled. ---
        let angle  = fastrand::f32() * std::f32::consts::TAU;
        let jitter = Vec2::new(angle.cos(), angle.sin()) * JITTER;

        // --- Semi-implicit Euler: update velocity, damp, then move. ---
        p.vel += (force + jitter) * dt;
        p.vel *= DAMPING;
        p.pos += p.vel * dt;

        // --- Reflect off plate edges so grains never escape. ---
        if p.pos.x < 0.0 { p.pos.x = 0.0; p.vel.x =  p.vel.x.abs(); }
        if p.pos.y < 0.0 { p.pos.y = 0.0; p.vel.y =  p.vel.y.abs(); }
        if p.pos.x > plate_w { p.pos.x = plate_w; p.vel.x = -p.vel.x.abs(); }
        if p.pos.y > plate_h { p.pos.y = plate_h; p.vel.y = -p.vel.y.abs(); }

        // --- Cache |Z| for colour mapping. ---
        p.displacement = chladni_z(p.pos.x, p.pos.y, m, n, plate_w, plate_h).abs();
    }
}
