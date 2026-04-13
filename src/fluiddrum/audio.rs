#![allow(dead_code, unused_imports, unused_variables)]

use std::sync::{Arc, Mutex};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::Stream;

// ─── Drum type ────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum DrumType {
    Kick,    // deep thud with pitch drop
    Snare,   // crispy snap: sine + noise burst
    Tom,     // mid resonant thump
    Cymbal,  // inharmonic shimmer, long decay
    Bell,    // pure ring, very long decay
    Bass,    // sub-bass rumble
}

impl DrumType {
    pub fn label(self) -> &'static str {
        match self {
            Self::Kick   => "Kick",
            Self::Snare  => "Snare",
            Self::Tom    => "Tom",
            Self::Cymbal => "Cymbal",
            Self::Bell   => "Bell",
            Self::Bass   => "Bass",
        }
    }

    pub const ALL: &'static [DrumType] = &[
        DrumType::Kick, DrumType::Snare, DrumType::Tom,
        DrumType::Cymbal, DrumType::Bell, DrumType::Bass,
    ];
}

// ─── Hit ─────────────────────────────────────────────────────────────────────

pub struct Hit {
    phase:        f32,
    freq:         f32,
    freq_target:  f32,   // for pitch-drop types (Kick, Tom)
    age:          f32,
    decay:        f32,
    amp:          f32,
    drum:         DrumType,
    // Up to 6 extra phases: harmonics (Cymbal, Bell) or noise partials (Snare)
    extra_phases: [f32; 6],
}

// Fixed inharmonic ratios for Cymbal (like a real cymbal's overtone series).
const CYMBAL_RATIOS: [f32; 6] = [1.0, 1.483, 1.932, 2.546, 3.087, 3.641];
// Noise partial frequencies for Snare (inharmonic, produces noise-like texture).
const SNARE_NOISE_HZ: [f32; 6] = [1200.0, 1543.0, 1876.0, 2310.0, 2743.0, 3187.0];

// ─── AudioState ───────────────────────────────────────────────────────────────

pub struct AudioState {
    pub hits:       Vec<Hit>,
    pub drag_freq:  f32,
    pub drag_amp:   f32,
    drag_phase:     f32,
}

impl AudioState {
    pub fn new() -> Self {
        Self {
            hits:       Vec::new(),
            drag_freq:  220.0,
            drag_amp:   0.0,
            drag_phase: 0.0,
        }
    }

    /// Trigger a drum hit. `base_freq` is the position-mapped fundamental (80–200 Hz).
    pub fn trigger_hit(&mut self, base_freq: f32, drum: DrumType) {
        if self.hits.len() >= 20 { return; }

        let (freq, freq_target, decay, amp) = match drum {
            DrumType::Kick   => (base_freq * 1.6, base_freq * 0.28, 12.0, 0.55),
            DrumType::Snare  => (base_freq * 1.1, base_freq * 1.1,  22.0, 0.40),
            DrumType::Tom    => (base_freq * 0.9, base_freq * 0.55,  7.0, 0.50),
            DrumType::Cymbal => (base_freq * 5.0, base_freq * 5.0,   1.2, 0.28),
            DrumType::Bell   => (base_freq * 3.0, base_freq * 3.0,   0.6, 0.40),
            DrumType::Bass   => (base_freq * 0.35, base_freq * 0.18, 6.0, 0.60),
        };

        // Pre-fill extra_phases with 0 (they advance in the audio callback).
        self.hits.push(Hit {
            phase: 0.0, freq, freq_target, age: 0.0, decay, amp,
            drum, extra_phases: [0.0; 6],
        });
    }

    pub fn set_drag(&mut self, freq: f32, amp: f32) {
        self.drag_freq = freq;
        self.drag_amp  = amp;
    }

    pub fn stop_drag(&mut self) {
        self.drag_amp = 0.0;
    }
}

// ─── Audio thread sample generation ──────────────────────────────────────────

fn write_samples_f32(
    data:         &mut [f32],
    channels:     usize,
    audio_state:  &Arc<Mutex<AudioState>>,
    sample_rate:  f32,
) {
    let mut state = match audio_state.try_lock() {
        Ok(s)  => s,
        Err(_) => { data.fill(0.0); return; }
    };

    let tau        = std::f32::consts::TAU;
    let frame_count = data.len() / channels;

    for frame in 0..frame_count {
        let mut sample = 0.0f32;
        let inv_sr = 1.0 / sample_rate;

        // ── Per-hit synthesis ─────────────────────────────────────────────────
        for hit in state.hits.iter_mut() {
            let env   = (-hit.age * hit.decay).exp();
            let t_01  = 1.0 - (-hit.age * 8.0).exp(); // 0→1 in ~0.1s for pitch glide

            // Pitch glide: freq → freq_target.
            let freq  = hit.freq + (hit.freq_target - hit.freq) * t_01;
            hit.phase += freq * tau * inv_sr;

            let s = match hit.drum {
                DrumType::Kick | DrumType::Tom => {
                    hit.phase.sin() * env * hit.amp
                }
                DrumType::Bass => {
                    // Slightly richer: add 2nd harmonic at 0.2×
                    (hit.phase.sin() + (hit.phase * 2.0).sin() * 0.2) * env * hit.amp
                }
                DrumType::Snare => {
                    // Sine body + noise burst from 6 inharmonic partials.
                    let body  = hit.phase.sin() * 0.35;
                    let noise: f32 = hit.extra_phases.iter()
                        .map(|p| p.sin())
                        .sum::<f32>() / 6.0;
                    (body + noise * 0.65) * env * hit.amp
                }
                DrumType::Cymbal => {
                    // Sum 6 inharmonic partials, each with slightly different decay.
                    let sum: f32 = hit.extra_phases.iter().enumerate()
                        .map(|(i, p)| {
                            let part_env = (-hit.age * hit.decay * (0.8 + i as f32 * 0.08)).exp();
                            p.sin() * part_env
                        })
                        .sum();
                    sum / 6.0 * hit.amp
                }
                DrumType::Bell => {
                    // Fundamental + 2.756× partial (bell's major seventh partial).
                    let fundamental = hit.phase.sin();
                    let partial     = hit.extra_phases[0].sin() * 0.5;
                    (fundamental + partial) * env * hit.amp
                }
            };
            sample += s;

            // Advance extra phases.
            match hit.drum {
                DrumType::Snare => {
                    for (i, p) in hit.extra_phases.iter_mut().enumerate() {
                        *p += SNARE_NOISE_HZ[i] * tau * inv_sr;
                    }
                }
                DrumType::Cymbal => {
                    let base = hit.freq;
                    for (i, p) in hit.extra_phases.iter_mut().enumerate() {
                        *p += base * CYMBAL_RATIOS[i] * tau * inv_sr;
                    }
                }
                DrumType::Bell => {
                    hit.extra_phases[0] += hit.freq * 2.756 * tau * inv_sr;
                }
                _ => {}
            }

            hit.age += inv_sr;
        }

        // Remove dead hits (envelope below ~0.5%).
        state.hits.retain(|h| (-h.age * h.decay).exp() > 0.005);

        // ── Drag theremin tone ────────────────────────────────────────────────
        sample += state.drag_phase.sin() * state.drag_amp * 0.18;
        state.drag_phase += state.drag_freq * tau * inv_sr;
        if state.drag_phase > tau { state.drag_phase -= tau; }

        // Soft-clip to prevent harsh clipping on dense hits.
        let sample = sample.tanh();

        for c in 0..channels {
            data[frame * channels + c] = sample;
        }
    }
}

// ─── Stream setup (generic over sample formats) ───────────────────────────────

pub fn setup_audio(audio_state: Arc<Mutex<AudioState>>) -> Option<Stream> {
    let host   = cpal::default_host();
    let device = host.default_output_device()?;
    let config = device.default_output_config().ok()?;
    let sr     = config.sample_rate().0 as f32;
    let ch     = config.channels() as usize;

    let build = |fmt| match fmt {
        cpal::SampleFormat::F32 => {
            let st = Arc::clone(&audio_state);
            device.build_output_stream(
                &config.clone().into(),
                move |data: &mut [f32], _| write_samples_f32(data, ch, &st, sr),
                |e| eprintln!("audio error: {e}"), None,
            )
        }
        cpal::SampleFormat::I16 => {
            let st = Arc::clone(&audio_state);
            device.build_output_stream(
                &config.clone().into(),
                move |data: &mut [i16], _| {
                    let mut buf = vec![0.0f32; data.len()];
                    write_samples_f32(&mut buf, ch, &st, sr);
                    for (d, s) in data.iter_mut().zip(&buf) {
                        *d = (s * i16::MAX as f32) as i16;
                    }
                },
                |e| eprintln!("audio error: {e}"), None,
            )
        }
        _ => {
            let st = Arc::clone(&audio_state);
            device.build_output_stream(
                &config.clone().into(),
                move |data: &mut [u16], _| {
                    let mut buf = vec![0.0f32; data.len()];
                    write_samples_f32(&mut buf, ch, &st, sr);
                    for (d, s) in data.iter_mut().zip(&buf) {
                        *d = ((s + 1.0) * 0.5 * u16::MAX as f32) as u16;
                    }
                },
                |e| eprintln!("audio error: {e}"), None,
            )
        }
    };

    build(config.sample_format()).ok()
}
