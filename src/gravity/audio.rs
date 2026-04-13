//! Audio engine: one sine-wave oscillator per planet (up to 4).
#![allow(dead_code, unused_variables, unused_imports)]

use std::sync::{Arc, Mutex};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

// ---------------------------------------------------------------------------
// AudioState – shared between the audio callback and the app thread
// ---------------------------------------------------------------------------

/// Per-planet tone descriptor updated by the app every frame.
#[derive(Clone, Debug)]
pub struct ToneDesc {
    pub frequency: f32,
    pub amplitude: f32,
}

/// State shared (via `Arc<Mutex<…>>`) between the main thread and the
/// audio callback. The callback calls `try_lock` so it never blocks.
pub struct AudioState {
    /// Active tones – at most 4 (one per planet).
    pub tones: Vec<ToneDesc>,
    /// Per-tone oscillator phase, kept in sync with `tones`.
    pub phases: Vec<f32>,
}

impl AudioState {
    pub fn new() -> Self {
        Self {
            tones: Vec::new(),
            phases: Vec::new(),
        }
    }

    /// Update the tone list from the app thread.
    ///
    /// Phases are preserved for tones whose frequency is close to an
    /// existing one so there is no audible click on small changes.
    pub fn set_tones(&mut self, new_tones: Vec<ToneDesc>) {
        // Grow or shrink the phase list to match.
        self.phases.resize(new_tones.len(), 0.0);
        self.tones = new_tones;
    }
}

impl Default for AudioState {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// AudioEngine – owns the cpal stream
// ---------------------------------------------------------------------------

/// Owns the live audio output stream.  Drop this to stop audio.
pub struct AudioEngine {
    /// Kept alive so the stream keeps running.
    _stream: cpal::Stream,
}

impl AudioEngine {
    /// Attempt to open the default output device and start streaming.
    ///
    /// Returns `None` if no audio device is available (common in CI / headless
    /// environments) so the app degrades gracefully.
    pub fn try_new(audio_state: Arc<Mutex<AudioState>>) -> Option<Self> {
        let host = cpal::default_host();
        let device = host.default_output_device()?;
        let config = device.default_output_config().ok()?;

        let sample_rate = config.sample_rate().0 as f32;
        let channels = config.channels() as usize;

        // Build the stream for the detected sample format.
        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => {
                build_stream::<f32>(&device, &config.into(), audio_state, sample_rate, channels)
            }
            cpal::SampleFormat::I16 => {
                build_stream::<i16>(&device, &config.into(), audio_state, sample_rate, channels)
            }
            cpal::SampleFormat::U16 => {
                build_stream::<u16>(&device, &config.into(), audio_state, sample_rate, channels)
            }
            _ => return None,
        }
        .ok()?;

        stream.play().ok()?;
        Some(Self { _stream: stream })
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Create a cpal output stream for a concrete sample type `T`.
fn build_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    audio_state: Arc<Mutex<AudioState>>,
    sample_rate: f32,
    channels: usize,
) -> Result<cpal::Stream, cpal::BuildStreamError>
where
    T: cpal::Sample + cpal::SizedSample + cpal::FromSample<f32>,
{
    use std::f32::consts::TAU;

    device.build_output_stream(
        config,
        move |output: &mut [T], _info: &cpal::OutputCallbackInfo| {
            // Try to grab the lock without blocking the audio thread.
            let mut state = match audio_state.try_lock() {
                Ok(g) => g,
                Err(_) => {
                    // Could not acquire – fill with silence and return.
                    for s in output.iter_mut() {
                        *s = T::from_sample(0.0f32);
                    }
                    return;
                }
            };

            let frames = output.len() / channels;

            // Snapshot tone descriptors so we can mutably borrow phases
            // without conflicting with the immutable borrow of tones.
            let tone_data: Vec<(f32, f32)> = state
                .tones
                .iter()
                .map(|t| (t.frequency, t.amplitude))
                .collect();

            for frame in 0..frames {
                let mut sample_val = 0.0f32;

                for (i, &(freq, amp)) in tone_data.iter().enumerate() {
                    if i < state.phases.len() {
                        sample_val += state.phases[i].sin() * amp * 0.12;
                        state.phases[i] += freq * TAU / sample_rate;
                        // Wrap phase to avoid precision loss over time.
                        if state.phases[i] > TAU {
                            state.phases[i] -= TAU;
                        }
                    }
                }

                // Soft clip to prevent clipping artefacts.
                sample_val = sample_val.tanh();

                let converted = T::from_sample(sample_val);
                for ch in 0..channels {
                    output[frame * channels + ch] = converted;
                }
            }
        },
        |err| {
            eprintln!("[audio] stream error: {err}");
        },
        None,
    )
}
