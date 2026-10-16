use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, StreamConfig};
use std::f32::consts::PI;
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub fn test_audio_output() -> Result<String, String> {
    let host = cpal::default_host();

    let device = host
        .default_output_device()
        .ok_or("No audio output device found")?;

    let supported_config = device.default_output_config().map_err(|e| e.to_string())?;

    let sample_format = supported_config.sample_format();
    let config: StreamConfig = supported_config.into();

    let sample_rate = config.sample_rate as f32;
    let channels = config.channels as usize;
    let phase = Arc::new(Mutex::new(0.0_f32));

    let make_sample = move |phase: &mut f32| {
        let sample = (*phase * 2.0 * PI).sin() * 0.12;

        *phase += 440.0 / sample_rate;

        if *phase >= 1.0 {
            *phase -= 1.0;
        }

        sample
    };

    let stream = match sample_format {
        SampleFormat::F32 => {
            let phase = Arc::clone(&phase);

            device.build_output_stream(
                config.clone(),
                move |data: &mut [f32], _| {
                    let mut phase = phase.lock().unwrap();

                    for frame in data.chunks_mut(channels) {
                        let sample = make_sample(&mut phase);

                        for output in frame {
                            *output = sample;
                        }
                    }
                },
                |error| eprintln!("Audio output error: {error}"),
                None,
            )
        }

        SampleFormat::I16 => {
            let phase = Arc::clone(&phase);

            device.build_output_stream(
                config.clone(),
                move |data: &mut [i16], _| {
                    let mut phase = phase.lock().unwrap();

                    for frame in data.chunks_mut(channels) {
                        let sample = make_sample(&mut phase);
                        let converted = (sample * i16::MAX as f32) as i16;

                        for output in frame {
                            *output = converted;
                        }
                    }
                },
                |error| eprintln!("Audio output error: {error}"),
                None,
            )
        }

        SampleFormat::U16 => {
            let phase = Arc::clone(&phase);

            device.build_output_stream(
                config.clone(),
                move |data: &mut [u16], _| {
                    let mut phase = phase.lock().unwrap();

                    for frame in data.chunks_mut(channels) {
                        let sample = make_sample(&mut phase);
                        let converted = ((sample + 1.0) * 0.5 * u16::MAX as f32) as u16;

                        for output in frame {
                            *output = converted;
                        }
                    }
                },
                |error| eprintln!("Audio output error: {error}"),
                None,
            )
        }

        other => {
            return Err(format!("Unsupported output sample format: {other:?}"));
        }
    }
    .map_err(|e| e.to_string())?;

    stream.play().map_err(|e| e.to_string())?;
    std::thread::sleep(Duration::from_secs(2));

    Ok("Native audio output test completed!".to_string())
}
