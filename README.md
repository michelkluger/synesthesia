# Synesthesia

A collection of audio-visual instruments and physics simulations — where sound becomes something you can see, touch, and explore. Built in Rust with [egui](https://github.com/emilk/egui).
*Made with Rust · egui · and a love for the intersection of physics and music.*


## Apps

### Cymatics
*The physics of sound made visible.*

Chladni patterns — sprinkle virtual sand on a vibrating plate and watch it migrate to the nodal lines where the plate is perfectly still. Every frequency produces a different geometric signature: stars, grids, mandalas. Change the mode numbers and watch the sand reorganise in real time.

<img width="1742" height="1003" alt="image" src="https://github.com/user-attachments/assets/f3faef17-5f6c-40b2-9640-b28b62734af9" />


**Physics:** The plate displacement follows the Chladni equation `Z(x,y) = cos(nπx) · cos(mπy) − cos(mπx) · cos(nπy)`. Sand particles experience a gradient force toward zero-displacement lines and accumulate there.

---

### Theremin
*Four octaves of pitch on an invisible canvas.*

Move your cursor (or your fingers) across the canvas to play. X controls pitch on an exponential frequency scale — A2 to A6, four full octaves. Y controls volume. Hold to sustain; hover quietly to explore.

<img width="1749" height="1003" alt="image" src="https://github.com/user-attachments/assets/120bed1e-1ada-4877-9903-d1614ab0fb14" />

**Multi-touch:** Place two fingers to play two simultaneous notes — full two-voice synthesis, independent oscillators, each finger leaving its own coloured trail.

<img width="1759" height="1019" alt="image" src="https://github.com/user-attachments/assets/f8dfe424-1da1-4654-a7c1-45ad157a2dad" />


**Tutorial mode:** Learn songs step-by-step with a guided overlay that shows you exactly where to place your hand. Or hit **▶** to watch the computer perform the song for you with an animated golden cursor.

Songs included:
- Ode to Joy — Beethoven
- Somewhere Over the Rainbow — Harold Arlen
- Good Vibrations — The Beach Boys
- Für Elise — Beethoven
- Interstellar Theme (Cornfield Chase) — Hans Zimmer
- Toccata & Fugue in D minor — J.S. Bach

Scales included: Pentatonic Minor, Blues, D Dorian, C Major, A Minor.

**Waveforms:** Sine · Sawtooth · Square

---

### Gravity Wells
*Orbits, slingshots, and gravitational music.*

Place planets anywhere on the canvas. Small particles are attracted by gravity, orbit, slingshot around multiple bodies, and occasionally collide. Each planet produces a tone whose pitch scales with its mass — the solar system as an instrument.

<img width="1600" height="948" alt="image" src="https://github.com/user-attachments/assets/b9287795-eb09-48da-99bd-adf8b2427f73" />


**Physics:** Newtonian `F = Gm₁m₂/r²` computed between every pair of bodies each frame. Particle trajectories are integrated with a fixed timestep; elastic collisions conserve momentum.

---

### Fluid Drum
*Hit a membrane and watch the wave spread.*

Click anywhere on the circular membrane to strike it. Waves propagate outward, reflect off the boundary, interfere with each other, and gradually decay — exactly as a real drum head behaves. Different strike positions excite different vibrational modes.

<img width="1739" height="1010" alt="image" src="https://github.com/user-attachments/assets/b30a06ac-fda7-4e35-808c-868c3dd26568" />


**Physics:** Solves the 2D wave equation `∂²u/∂t² = c²·∇²u` on a discrete grid using finite differences. The wavespeed `c` and decay rate are adjustable — from a tight snare to a vast gong.

---

## Building

```bash
git clone https://github.com/michelkluger/synesthesia
cd synesthesia
cargo run --release
```

Requires Rust stable (1.75+). Audio via [cpal](https://github.com/RustAudio/cpal) — no extra setup on Windows or macOS.

---

## Stack

| Crate | Role |
|-------|------|
| [eframe 0.29](https://github.com/emilk/egui/tree/master/crates/eframe) | Window, event loop |
| [egui 0.29](https://github.com/emilk/egui) | Immediate-mode UI and 2D painting |
| [cpal 0.15](https://github.com/RustAudio/cpal) | Cross-platform audio output |
| [glam 0.29](https://github.com/bitshifter/glam-rs) | Vector math |
| [fastrand 2](https://github.com/smol-rs/fastrand) | Fast noise and particle randomisation |
