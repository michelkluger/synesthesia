#![allow(dead_code, unused)]

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, PartialEq)]
pub enum Waveform {
    Sine,
    Sawtooth,
    Square,
}

pub struct AudioState {
    // ── Voice 1 (primary pointer / first touch) ──────────────────────────────
    pub target_freq: f32,
    pub target_vol:  f32,
    pub active:      bool,
    pub waveform:    Waveform,
    pub current_freq: f32,
    pub current_vol:  f32,
    pub phase:  f32,
    pub phase2: f32,
    // ── Voice 2 (second touch / second hand) ─────────────────────────────────
    pub target_freq2: f32,
    pub target_vol2:  f32,
    pub active2:      bool,
    pub current_freq2: f32,
    pub current_vol2:  f32,
    pub phase3: f32,
    pub phase4: f32,
}

impl Default for AudioState {
    fn default() -> Self {
        Self {
            target_freq: 440.0,
            target_vol: 0.0,
            active: false,
            waveform: Waveform::Sine,
            current_freq: 440.0,
            current_vol: 0.0,
            phase: 0.0,
            phase2: 0.0,
            target_freq2: 440.0,
            target_vol2: 0.0,
            active2: false,
            current_freq2: 440.0,
            current_vol2: 0.0,
            phase3: 0.0,
            phase4: 0.0,
        }
    }
}

pub struct AudioEngine {
    _stream: Option<cpal::Stream>,
    pub state: Arc<Mutex<AudioState>>,
}

impl AudioEngine {
    pub fn new() -> Self {
        let state = Arc::new(Mutex::new(AudioState::default()));

        let stream = match Self::build_stream(Arc::clone(&state)) {
            Ok(s) => {
                if let Err(e) = s.play() {
                    eprintln!("[audio] failed to play stream: {e}");
                }
                Some(s)
            }
            Err(e) => {
                eprintln!("[audio] cpal init failed, running silent: {e}");
                None
            }
        };

        Self { _stream: stream, state }
    }

    fn build_stream(state: Arc<Mutex<AudioState>>) -> Result<cpal::Stream, Box<dyn std::error::Error>> {
        let host   = cpal::default_host();
        let device = host.default_output_device().ok_or("no output device")?;
        let config = device.default_output_config()?;
        let sample_rate = config.sample_rate().0 as f32;

        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 =>
                Self::build_typed_stream::<f32>(&device, &config.into(), sample_rate, state)?,
            cpal::SampleFormat::I16 =>
                Self::build_typed_stream::<i16>(&device, &config.into(), sample_rate, state)?,
            cpal::SampleFormat::U16 =>
                Self::build_typed_stream::<u16>(&device, &config.into(), sample_rate, state)?,
            fmt => return Err(format!("unsupported sample format: {fmt:?}").into()),
        };

        Ok(stream)
    }

    fn build_typed_stream<T>(
        device:      &cpal::Device,
        config:      &cpal::StreamConfig,
        sample_rate: f32,
        state:       Arc<Mutex<AudioState>>,
    ) -> Result<cpal::Stream, Box<dyn std::error::Error>>
    where
        T: cpal::Sample + cpal::SizedSample + cpal::FromSample<f32>,
    {
        let channels = config.channels as usize;

        // ── Voice 1 locals ────────────────────────────────────────────────────
        let mut local_freq    = 440.0f32;
        let mut local_vol     = 0.0f32;
        let mut local_phase   = 0.0f32;
        let mut local_phase2  = 0.0f32;
        let mut local_waveform = Waveform::Sine;
        let mut local_active  = false;

        // ── Voice 2 locals ────────────────────────────────────────────────────
        let mut local_freq2   = 440.0f32;
        let mut local_vol2    = 0.0f32;
        let mut local_phase3  = 0.0f32;
        let mut local_phase4  = 0.0f32;
        let mut local_active2 = false;

        let stream = device.build_output_stream(
            config,
            move |data: &mut [T], _info: &cpal::OutputCallbackInfo| {
                // Sync shared state once per callback (never block)
                if let Ok(mut s) = state.try_lock() {
                    local_active   = s.active;
                    local_active2  = s.active2;
                    local_waveform = s.waveform;
                    s.current_freq  = local_freq;
                    s.current_vol   = local_vol;
                    s.current_freq2 = local_freq2;
                    s.current_vol2  = local_vol2;
                }

                let two_pi = std::f32::consts::TAU;

                for frame in data.chunks_mut(channels) {
                    // ── Portamento glide ─────────────────────────────────────
                    let (tf, tv, tf2, tv2) = if let Ok(s) = state.try_lock() {
                        (
                            s.target_freq,
                            if s.active  { s.target_vol  } else { 0.0 },
                            s.target_freq2,
                            if s.active2 { s.target_vol2 } else { 0.0 },
                        )
                    } else {
                        (
                            local_freq,
                            if local_active  { local_vol  } else { 0.0 },
                            local_freq2,
                            if local_active2 { local_vol2 } else { 0.0 },
                        )
                    };

                    local_freq  += (tf  - local_freq)  * 0.005;
                    local_vol   += (tv  - local_vol)   * 0.01;
                    local_freq2 += (tf2 - local_freq2) * 0.005;
                    local_vol2  += (tv2 - local_vol2)  * 0.01;

                    // ── Advance phases ───────────────────────────────────────
                    local_phase  += local_freq  * two_pi / sample_rate;
                    local_phase2 += local_freq  * 2.0 * two_pi / sample_rate;
                    local_phase3 += local_freq2 * two_pi / sample_rate;
                    local_phase4 += local_freq2 * 2.0 * two_pi / sample_rate;

                    // Wrap to avoid float precision loss
                    let wrap = two_pi * 1000.0;
                    if local_phase  > wrap { local_phase  -= wrap; }
                    if local_phase2 > wrap { local_phase2 -= wrap; }
                    if local_phase3 > wrap { local_phase3 -= wrap; }
                    if local_phase4 > wrap { local_phase4 -= wrap; }

                    // ── Synthesise each voice ────────────────────────────────
                    let v1 = synthesise(local_waveform, local_phase, local_phase2, local_vol);
                    let v2 = synthesise(local_waveform, local_phase3, local_phase4, local_vol2);

                    // Mix — each voice at 0.18 max; sum stays headroom-safe
                    let sample = v1 + v2;

                    let out = T::from_sample(sample);
                    for ch in frame.iter_mut() { *ch = out; }
                }
            },
            |err| eprintln!("[audio] stream error: {err}"),
            None,
        )?;

        Ok(stream)
    }
}

// ─── Waveform synthesis ───────────────────────────────────────────────────────

#[inline]
fn synthesise(waveform: Waveform, phase: f32, phase2: f32, vol: f32) -> f32 {
    if vol < 0.0001 { return 0.0; }
    let raw = match waveform {
        Waveform::Sine => {
            phase.sin() + phase2.sin() * 0.15
        }
        Waveform::Sawtooth => {
            let mut s = 0.0f32;
            for n in 1..=8u32 {
                s += (n as f32 * phase).sin() / n as f32;
            }
            s * (2.0 / std::f32::consts::PI)
        }
        Waveform::Square => {
            let mut s = 0.0f32;
            for n in (1..=15u32).step_by(2) {
                s += (n as f32 * phase).sin() / n as f32;
            }
            s * (4.0 / std::f32::consts::PI)
        }
    };
    raw * vol * 0.18
}
