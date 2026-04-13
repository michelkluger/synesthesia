//! Audio engine: generates a continuous sine wave at the current Chladni frequency.
//! If audio initialization fails (no output device, driver error, etc.) we print
//! a warning and run silently — the app never crashes due to audio problems.

use std::sync::{Arc, Mutex};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

// ---------------------------------------------------------------------------
// Shared state (read by the audio thread, written by the UI thread)
// ---------------------------------------------------------------------------

pub struct AudioState {
    pub frequency: f32, // Hz — updated whenever the slider moves
    pub volume: f32,    // 0.0 – 1.0
    pub phase: f32,     // current sine phase accumulator (radians)
}

impl AudioState {
    fn new() -> Self {
        Self {
            frequency: 220.0,
            volume: 0.4,
            phase: 0.0,
        }
    }
}

// ---------------------------------------------------------------------------
// Engine handle kept alive for the duration of the app
// ---------------------------------------------------------------------------

pub struct AudioEngine {
    /// Keeping the stream alive; dropping it stops playback.
    _stream: cpal::Stream,
    /// Shared mutable state between the UI and the audio callback.
    pub state: Arc<Mutex<AudioState>>,
}

impl AudioEngine {
    /// Try to create and start an audio stream.  Returns `None` on any error.
    pub fn try_new() -> Option<Self> {
        let host = cpal::default_host();

        let device = match host.default_output_device() {
            Some(d) => d,
            None => {
                eprintln!("[audio] No output device found — running silently.");
                return None;
            }
        };

        let config = match device.default_output_config() {
            Ok(c) => c,
            Err(e) => {
                eprintln!("[audio] Could not get default output config: {e} — running silently.");
                return None;
            }
        };

        let state = Arc::new(Mutex::new(AudioState::new()));
        let state_cb = Arc::clone(&state);

        let sample_rate = config.sample_rate().0 as f32;
        let channels = config.channels() as usize;

        // Build the stream with the detected sample format.
        let stream_result = match config.sample_format() {
            cpal::SampleFormat::F32 => build_stream::<f32>(&device, &config.into(), state_cb, sample_rate, channels),
            cpal::SampleFormat::I16 => build_stream::<i16>(&device, &config.into(), state_cb, sample_rate, channels),
            cpal::SampleFormat::U16 => build_stream::<u16>(&device, &config.into(), state_cb, sample_rate, channels),
            _ => {
                eprintln!("[audio] Unsupported sample format — running silently.");
                return None;
            }
        };

        match stream_result {
            Ok(stream) => {
                if let Err(e) = stream.play() {
                    eprintln!("[audio] Could not start stream: {e} — running silently.");
                    return None;
                }
                Some(AudioEngine { _stream: stream, state })
            }
            Err(e) => {
                eprintln!("[audio] Could not build stream: {e} — running silently.");
                None
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Generic stream builder
// ---------------------------------------------------------------------------

fn build_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    state: Arc<Mutex<AudioState>>,
    sample_rate: f32,
    channels: usize,
) -> Result<cpal::Stream, cpal::BuildStreamError>
where
    T: cpal::Sample + cpal::SizedSample + cpal::FromSample<f32>,
{
    device.build_output_stream(
        config,
        move |data: &mut [T], _info: &cpal::OutputCallbackInfo| {
            // Lock as briefly as possible — read frequency/volume, advance phase.
            let (vol, start_phase, phase_inc) = {
                if let Ok(mut s) = state.try_lock() {
                    let two_pi = std::f32::consts::TAU;
                    let phase_inc = freq_to_phase_inc(s.frequency, sample_rate, two_pi);
                    let frame_count = data.len() / channels;
                    let start = s.phase;
                    let vol = s.volume;
                    s.phase = (s.phase + phase_inc * frame_count as f32) % two_pi;
                    (vol, start, phase_inc)
                } else {
                    // Lock contention: output silence.
                    for sample in data.iter_mut() {
                        *sample = T::from_sample(0.0_f32);
                    }
                    return;
                }
            };

            // Fill the buffer frame by frame.
            let mut ph = start_phase;
            for frame in data.chunks_exact_mut(channels) {
                let sample_f32 = ph.sin() * vol * 0.15;
                ph += phase_inc;
                for out in frame.iter_mut() {
                    *out = T::from_sample(sample_f32);
                }
            }
        },
        |err| eprintln!("[audio] Stream error: {err}"),
        None,
    )
}

#[inline]
fn freq_to_phase_inc(freq: f32, sample_rate: f32, two_pi: f32) -> f32 {
    freq / sample_rate * two_pi
}
