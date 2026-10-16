use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, Stream, StreamConfig};
use serde::Serialize;
use std::sync::{Arc, Mutex};

use super::preview::decode_preview;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PlaybackStatus {
    Stopped,
    Playing,
    Paused,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlayerState {
    pub status: PlaybackStatus,
    pub position: f64,
    pub duration: f64,
    pub volume: f32,

    #[serde(skip)]
    cursor: usize,

    #[serde(skip)]
    sample_rate: u32,
}

impl Default for PlayerState {
    fn default() -> Self {
        Self {
            status: PlaybackStatus::Stopped,
            position: 0.0,
            duration: 0.0,
            volume: 0.65,
            cursor: 0,
            sample_rate: 0,
        }
    }
}

pub struct AudioPlayer {
    state: Arc<Mutex<PlayerState>>,
    stream: Mutex<Option<Stream>>,
}

impl AudioPlayer {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(PlayerState::default())),
            stream: Mutex::new(None),
        }
    }

    pub fn get_state(&self) -> Result<PlayerState, String> {
        Ok(self.state.lock().map_err(|e| e.to_string())?.clone())
    }

    pub fn load(&self, path: &str) -> Result<(), String> {
        let host = cpal::default_host();

        let device = host
            .default_output_device()
            .ok_or("No audio output device found")?;

        let supported = device.default_output_config().map_err(|e| e.to_string())?;

        let format = supported.sample_format();
        let config: StreamConfig = supported.into();

        if config.channels == 0 || config.sample_rate == 0 {
            return Err("Invalid audio output configuration".into());
        }
        let pcm = Arc::new(decode_preview(path, config.sample_rate)?);
        eprintln!(
            "[Resonance] Loaded {} | {:.2} seconds | {} samples",
            path,
            pcm.len() as f64 / (config.sample_rate as f64 * 2.0),
            pcm.len()
        );
        if pcm.is_empty() {
            return Err("FFmpeg returned no audio samples".into());
        }

        let duration = pcm.len() as f64 / (config.sample_rate as f64 * 2.0);
        let mut stream_slot = self.stream.lock().map_err(|e| e.to_string())?;

        *stream_slot = None;

        {
            let mut state = self.state.lock().map_err(|e| e.to_string())?;

            state.status = PlaybackStatus::Stopped;
            state.cursor = 0;
            state.position = 0.0;
            state.duration = duration;
            state.sample_rate = config.sample_rate;
        }

        let channels = config.channels as usize;

        fn render<T>(
            data: &mut [T],
            channels: usize,
            pcm: &[f32],
            state: &Arc<Mutex<PlayerState>>,
            mut convert: impl FnMut(f32) -> T,
        ) {
            let Ok(mut player) = state.lock() else {
                for output in data {
                    *output = convert(0.0);
                }
                return;
            };

            for frame in data.chunks_mut(channels) {
                let playing = player.status == PlaybackStatus::Playing;
                let index = player.cursor * 2;

                let left = if playing {
                    pcm.get(index).copied().unwrap_or(0.0)
                } else {
                    0.0
                };

                let right = if playing {
                    pcm.get(index + 1).copied().unwrap_or(0.0)
                } else {
                    0.0
                };

                for (channel, output) in frame.iter_mut().enumerate() {
                    let sample = if channels == 1 {
                        (left + right) * 0.5
                    } else if channel == 0 {
                        left
                    } else if channel == 1 {
                        right
                    } else {
                        0.0
                    };

                    *output = convert(sample * player.volume);
                }

                if playing {
                    player.cursor += 1;

                    player.position = player.cursor as f64 / player.sample_rate as f64;

                    if player.cursor * 2 >= pcm.len() {
                        player.status = PlaybackStatus::Stopped;
                        player.position = player.duration;
                    }
                }
            }
        }
        let stream = match format {
            SampleFormat::F32 => {
                let pcm = Arc::clone(&pcm);
                let state = Arc::clone(&self.state);

                device.build_output_stream(
                    config.clone(),
                    move |data: &mut [f32], _| {
                        render(data, channels, &pcm, &state, |x| x.clamp(-1.0, 1.0));
                    },
                    |e| eprintln!("CPAL error: {e}"),
                    None,
                )
            }

            SampleFormat::I16 => {
                let pcm = Arc::clone(&pcm);
                let state = Arc::clone(&self.state);

                device.build_output_stream(
                    config.clone(),
                    move |data: &mut [i16], _| {
                        render(data, channels, &pcm, &state, |x| {
                            (x.clamp(-1.0, 1.0) * i16::MAX as f32) as i16
                        });
                    },
                    |e| eprintln!("CPAL error: {e}"),
                    None,
                )
            }

            SampleFormat::U16 => {
                let pcm = Arc::clone(&pcm);
                let state = Arc::clone(&self.state);

                device.build_output_stream(
                    config.clone(),
                    move |data: &mut [u16], _| {
                        render(data, channels, &pcm, &state, |x| {
                            ((x.clamp(-1.0, 1.0) + 1.0) * 0.5 * u16::MAX as f32) as u16
                        });
                    },
                    |e| eprintln!("CPAL error: {e}"),
                    None,
                )
            }

            other => {
                return Err(format!("Unsupported output sample format: {other:?}"));
            }
        }
        .map_err(|e| e.to_string())?;

        // Keep the stream alive after this function returns.
        *stream_slot = Some(stream);

        Ok(())
    }

    pub fn play(&self) -> Result<(), String> {
        let stream_slot = self.stream.lock().map_err(|e| e.to_string())?;

        let stream = stream_slot.as_ref().ok_or("Load a track first")?;

        {
            let mut state = self.state.lock().map_err(|e| e.to_string())?;

            if state.cursor as f64 / state.sample_rate as f64 >= state.duration {
                state.cursor = 0;
                state.position = 0.0;
            }

            state.status = PlaybackStatus::Playing;
        }

        stream.play().map_err(|e| e.to_string())
    }

    pub fn pause(&self) -> Result<(), String> {
        let mut state = self.state.lock().map_err(|e| e.to_string())?;

        if state.status == PlaybackStatus::Playing {
            state.status = PlaybackStatus::Paused;
        }

        Ok(())
    }

    pub fn resume(&self) -> Result<(), String> {
        self.play()
    }

    pub fn stop(&self) -> Result<(), String> {
        let mut state = self.state.lock().map_err(|e| e.to_string())?;

        state.status = PlaybackStatus::Stopped;
        state.cursor = 0;
        state.position = 0.0;

        Ok(())
    }

    pub fn set_volume(&self, volume: f32) -> Result<(), String> {
        let mut state = self.state.lock().map_err(|e| e.to_string())?;

        state.volume = volume.clamp(0.0, 1.0);
        Ok(())
    }
    pub fn seek(&self, seconds: f64) -> Result<(), String> {
        if !seconds.is_finite() {
            return Err("Invalid seek position".into());
        }

        let mut state = self.state.lock().map_err(|e| e.to_string())?;

        if state.sample_rate == 0 {
            return Err("No track loaded".into());
        }

        let target = seconds.clamp(0.0, state.duration);

        state.cursor = (target * state.sample_rate as f64) as usize;

        state.position = target;

        Ok(())
    }
}
