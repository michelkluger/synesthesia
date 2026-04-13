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
    pub target_freq: f32,
    pub target_vol: f32,
    pub active: bool,
    pub waveform: Waveform,
    // internal — only touched by audio thread
    pub current_freq: f32,
    pub current_vol: f32,
    pub phase: f32,
    pub phase2: f32,
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

        Self {
            _stream: stream,
            state,
        }
    }

    fn build_stream(state: Arc<Mutex<AudioState>>) -> Result<cpal::Stream, Box<dyn std::error::Error>> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or("no output device")?;

        let config = device.default_output_config()?;
        let sample_rate = config.sample_rate().0 as f32;

        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => {
                Self::build_typed_stream::<f32>(&device, &config.into(), sample_rate, state)?
            }
            cpal::SampleFormat::I16 => {
                Self::build_typed_stream::<i16>(&device, &config.into(), sample_rate, state)?
            }
            cpal::SampleFormat::U16 => {
                Self::build_typed_stream::<u16>(&device, &config.into(), sample_rate, state)?
            }
            fmt => {
                return Err(format!("unsupported sample format: {fmt:?}").into());
            }
        };

        Ok(stream)
    }

    fn build_typed_stream<T>(
        device: &cpal::Device,
        config: &cpal::StreamConfig,
        sample_rate: f32,
        state: Arc<Mutex<AudioState>>,
    ) -> Result<cpal::Stream, Box<dyn std::error::Error>>
    where
        T: cpal::Sample + cpal::SizedSample + cpal::FromSample<f32>,
    {
        let channels = config.channels as usize;

        // Local copies for the audio thread (no mutex per-sample after try_lock fails)
        let mut local_freq = 440.0f32;
        let mut local_vol = 0.0f32;
        let mut local_phase = 0.0f32;
        let mut local_phase2 = 0.0f32;
        let mut local_waveform = Waveform::Sine;
        let mut local_active = false;

        let stream = device.build_output_stream(
            config,
            move |data: &mut [T], _info: &cpal::OutputCallbackInfo| {
                // Try to sync with UI state — never block
                if let Ok(mut s) = state.try_lock() {
                    local_active = s.active;
                    local_waveform = s.waveform;
                    // Write back portamento state so UI can read current_freq if desired
                    s.current_freq = local_freq;
                    s.current_vol = local_vol;
                }

                for frame in data.chunks_mut(channels) {
                    // Portamento glide
                    let (tf, tv) = if let Ok(s) = state.try_lock() {
                        (s.target_freq, if s.active { s.target_vol } else { 0.0 })
                    } else {
                        (local_freq, if local_active { local_vol } else { 0.0 })
                    };

                    local_freq += (tf - local_freq) * 0.005;
                    local_vol += (tv - local_vol) * 0.01;

                    let two_pi = std::f32::consts::TAU;
                    local_phase += local_freq * two_pi / sample_rate;
                    local_phase2 += local_freq * 2.0 * two_pi / sample_rate;

                    // Wrap phases
                    if local_phase > two_pi * 1000.0 {
                        local_phase -= two_pi * 1000.0;
                    }
                    if local_phase2 > two_pi * 1000.0 {
                        local_phase2 -= two_pi * 1000.0;
                    }

                    let sample = match local_waveform {
                        Waveform::Sine => {
                            (local_phase.sin() + local_phase2.sin() * 0.15) * local_vol * 0.18
                        }
                        Waveform::Sawtooth => {
                            // Sawtooth via additive harmonics (first 8)
                            let mut s = 0.0f32;
                            let mut ph = local_phase;
                            for n in 1..=8u32 {
                                s += (n as f32 * ph).sin() / n as f32;
                                ph = local_phase; // use fundamental phase for each
                            }
                            // Renormalize sawtooth peak to ~1
                            s * (2.0 / std::f32::consts::PI) * local_vol * 0.18
                        }
                        Waveform::Square => {
                            // Square via additive odd harmonics
                            let mut s = 0.0f32;
                            for n in (1..=15u32).step_by(2) {
                                s += (n as f32 * local_phase).sin() / n as f32;
                            }
                            s * (4.0 / std::f32::consts::PI) * local_vol * 0.18
                        }
                    };

                    let out = T::from_sample(sample);
                    for ch in frame.iter_mut() {
                        *ch = out;
                    }
                }
            },
            |err| eprintln!("[audio] stream error: {err}"),
            None,
        )?;

        Ok(stream)
    }
}
