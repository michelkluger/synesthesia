//! Physics simulation: planets, particles, gravitational integration.
#![allow(dead_code, unused_variables, unused_imports)]

use std::collections::VecDeque;
use std::f32::consts::TAU;
use glam::Vec2;

/// Gravitational constant used in the simulation.
pub const G: f32 = 500.0;

/// Maximum number of trail points kept per particle.
pub const TRAIL_MAX: usize = 60;

/// Maximum particle speed (px/s) — prevents runaway velocities.
pub const MAX_SPEED: f32 = 800.0;

/// Softening radius to avoid singularity at zero distance.
pub const SOFTENING: f32 = 20.0;

// ---------------------------------------------------------------------------
// Planet
// ---------------------------------------------------------------------------

/// A gravitational body placed on the canvas by the user.
#[derive(Clone)]
pub struct Planet {
    /// Canvas position in pixels.
    pub pos: Vec2,
    /// Mass in arbitrary units (500–5000).
    pub mass: f32,
    /// Hue value in [0, 1] assigned at spawn for consistent colouring.
    pub hue: f32,
    /// Musical frequency in Hz derived from mass.
    pub freq: f32,
}

impl Planet {
    /// Create a new planet with a given position, mass, and hue.
    pub fn new(pos: Vec2, mass: f32, hue: f32) -> Self {
        let freq = mass_to_freq(mass);
        Self { pos, mass, hue, freq }
    }

    /// Visual radius in pixels, scales with the square-root of mass.
    pub fn radius(&self) -> f32 {
        (self.mass / 500.0).sqrt() * 8.0
    }
}

/// Map planet mass → musical frequency (110 Hz – 880 Hz).
pub fn mass_to_freq(mass: f32) -> f32 {
    110.0 * 2.0_f32.powf((mass - 500.0) / 4500.0 * 3.0)
}

// ---------------------------------------------------------------------------
// Particle
// ---------------------------------------------------------------------------

/// A massless test particle that orbits the planets.
#[derive(Clone)]
pub struct Particle {
    /// Current canvas position.
    pub pos: Vec2,
    /// Current velocity in px/s.
    pub vel: Vec2,
    /// Time alive in seconds.
    pub age: f32,
    /// Recent positions used to draw the glowing trail.
    pub trail: VecDeque<Vec2>,
}

impl Particle {
    /// Construct a particle with a given position and velocity.
    pub fn new(pos: Vec2, vel: Vec2) -> Self {
        Self {
            pos,
            vel,
            age: 0.0,
            trail: VecDeque::with_capacity(TRAIL_MAX + 1),
        }
    }
}

// ---------------------------------------------------------------------------
// Simulation
// ---------------------------------------------------------------------------

/// Total number of particles kept in the simulation at all times.
pub const PARTICLE_COUNT: usize = 800;

/// Advance the physics by one timestep `dt` seconds.
///
/// # Arguments
/// * `particles` - mutable slice of all particles
/// * `planets`   - current planet list
/// * `dt`        - timestep (seconds)
/// * `bounds`    - canvas size (width, height) for off-screen detection
pub fn step(particles: &mut Vec<Particle>, planets: &[Planet], dt: f32, bounds: Vec2) {
    if planets.is_empty() {
        // No gravity source — just age particles and let them drift.
        for p in particles.iter_mut() {
            p.pos += p.vel * dt;
            push_trail(p);
            p.age += dt;
        }
        return;
    }

    for p in particles.iter_mut() {
        // ---- Gravitational acceleration --------------------------------
        let mut accel = Vec2::ZERO;
        let mut nearest_idx = 0usize;
        let mut nearest_dist = f32::MAX;

        for (i, planet) in planets.iter().enumerate() {
            let delta = planet.pos - p.pos;
            let dist = delta.length().max(SOFTENING);
            let a_mag = G * planet.mass / (dist * dist);
            accel += delta.normalize() * a_mag;

            if dist < nearest_dist {
                nearest_dist = dist;
                nearest_idx = i;
            }
        }

        // ---- Absorption / respawn near planet surface ------------------
        let planet_r = planets[nearest_idx].radius();
        if nearest_dist < planet_r + 2.0 {
            spawn_orbit(p, &planets[nearest_idx]);
            continue;
        }

        // ---- Velocity-Verlet integration --------------------------------
        p.vel += accel * dt;

        // Clamp speed to avoid explosions.
        let speed = p.vel.length();
        if speed > MAX_SPEED {
            p.vel = p.vel / speed * MAX_SPEED;
        }

        p.pos += p.vel * dt;

        // ---- Off-screen respawn ----------------------------------------
        let margin = 50.0;
        if p.pos.x < -margin
            || p.pos.x > bounds.x + margin
            || p.pos.y < -margin
            || p.pos.y > bounds.y + margin
        {
            let idx = fastrand::usize(..planets.len());
            spawn_orbit(p, &planets[idx]);
            continue;
        }

        // ---- Trail & age -----------------------------------------------
        push_trail(p);
        p.age += dt;
    }
}

/// Append the current position to the particle's trail, capping at TRAIL_MAX.
fn push_trail(p: &mut Particle) {
    p.trail.push_back(p.pos);
    if p.trail.len() > TRAIL_MAX {
        p.trail.pop_front();
    }
}

/// Reset a particle to a stable circular orbit around `planet`.
pub fn spawn_orbit(p: &mut Particle, planet: &Planet) {
    let r = planet.radius() * (1.5 + fastrand::f32() * 3.0);
    let angle = fastrand::f32() * TAU;
    let (sin_a, cos_a) = angle.sin_cos();

    p.pos = planet.pos + Vec2::new(cos_a, sin_a) * r;

    let orbital_speed = (G * planet.mass / r).sqrt();
    // Perpendicular direction for a prograde orbit, with slight speed variation.
    p.vel = Vec2::new(-sin_a, cos_a) * orbital_speed * (0.8 + fastrand::f32() * 0.4);

    p.trail.clear();
    p.age = 0.0;
}

/// Seed the particle list with stable circular orbits split across all planets.
pub fn seed_particles(planets: &[Planet]) -> Vec<Particle> {
    let mut out = Vec::with_capacity(PARTICLE_COUNT);
    if planets.is_empty() {
        // Particles will be properly initialised once a planet is added.
        for _ in 0..PARTICLE_COUNT {
            out.push(Particle::new(Vec2::ZERO, Vec2::ZERO));
        }
        return out;
    }
    for i in 0..PARTICLE_COUNT {
        let planet = &planets[i % planets.len()];
        let mut p = Particle::new(Vec2::ZERO, Vec2::ZERO);
        spawn_orbit(&mut p, planet);
        out.push(p);
    }
    out
}
