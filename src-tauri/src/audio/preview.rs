use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, StreamConfig};
use ffmpeg_next as ffmpeg;

use std::sync::Arc;
use std::time::Duration;

pub(crate) fn decode_preview(path: &str, output_rate: u32) -> Result<Vec<f32>, String> {
    if output_rate == 0 {
        return Err("Invalid output sample rate".into());
    }

    ffmpeg::init().map_err(|e| e.to_string())?;

    let mut input = ffmpeg::format::input(path).map_err(|e| e.to_string())?;

    let stream = input
        .streams()
        .best(ffmpeg::media::Type::Audio)
        .ok_or("No audio stream found")?;

    let stream_index = stream.index();

    let context = ffmpeg::codec::context::Context::from_parameters(stream.parameters())
        .map_err(|e| e.to_string())?;

    let mut decoder = context.decoder().audio().map_err(|e| e.to_string())?;

    let input_rate = decoder.rate();

    let mut resampler = ffmpeg::software::resampling::Context::get(
        decoder.format(),
        decoder.channel_layout(),
        input_rate,
        ffmpeg::format::Sample::F32(ffmpeg::format::sample::Type::Packed),
        ffmpeg::channel_layout::ChannelLayout::STEREO,
        output_rate,
    )
    .map_err(|e| e.to_string())?;

    let mut pcm = Vec::<f32>::new();

    let mut decoded = ffmpeg::frame::Audio::empty();

    let mut append_frame = |decoded: &ffmpeg::frame::Audio| -> Result<(), String> {
        let capacity = ((decoded.samples() as u64 * output_rate as u64 + input_rate as u64 - 1)
            / input_rate as u64) as usize
            + 256;

        let mut converted = ffmpeg::frame::Audio::new(
            ffmpeg::format::Sample::F32(ffmpeg::format::sample::Type::Packed),
            capacity,
            ffmpeg::channel_layout::ChannelLayout::STEREO,
        );

        converted.set_rate(output_rate);

        resampler
            .run(decoded, &mut converted)
            .map_err(|e| e.to_string())?;

        if converted.samples() > 0 {
            let valid_len = converted.samples() * 2;
            let data = converted.plane::<f32>(0);

            pcm.extend_from_slice(&data[..valid_len]);
        }

        Ok(())
    };

    for (packet_stream, packet) in input.packets() {
        if packet_stream.index() != stream_index {
            continue;
        }

        decoder.send_packet(&packet).map_err(|e| e.to_string())?;

        while decoder.receive_frame(&mut decoded).is_ok() {
            append_frame(&decoded)?;
        }
    }

    decoder.send_eof().map_err(|e| e.to_string())?;

    while decoder.receive_frame(&mut decoded).is_ok() {
        append_frame(&decoded)?;
    }

    drop(append_frame);

    loop {
        let mut tail = ffmpeg::frame::Audio::new(
            ffmpeg::format::Sample::F32(ffmpeg::format::sample::Type::Packed),
            4096,
            ffmpeg::channel_layout::ChannelLayout::STEREO,
        );

        tail.set_rate(output_rate);

        resampler.flush(&mut tail).map_err(|e| e.to_string())?;

        if tail.samples() == 0 {
            break;
        }

        let valid_len = tail.samples() * 2;
        let data = tail.plane::<f32>(0);

        pcm.extend_from_slice(&data[..valid_len]);
    }

    if pcm.is_empty() {
        return Err("FFmpeg decoded no audio samples".into());
    }

    Ok(pcm)
}
pub fn play_preview(path: &str) -> Result<String, String> {
    let host = cpal::default_host();

    let device = host
        .default_output_device()
        .ok_or("No audio output device found")?;

    let supported = device.default_output_config().map_err(|e| e.to_string())?;

    let sample_format = supported.sample_format();
    let config: StreamConfig = supported.into();

    let pcm = Arc::new(decode_preview(path, config.sample_rate)?);

    let channels = config.channels as usize;

    if channels == 0 {
        return Err("Audio device reported zero channels".into());
    }
    fn sample_at(pcm: &[f32], output_position: usize, channels: usize) -> f32 {
        let frame_index = output_position / channels;
        let channel = output_position % channels;

        let left = pcm.get(frame_index * 2).copied().unwrap_or(0.0);

        let right = pcm.get(frame_index * 2 + 1).copied().unwrap_or(0.0);

        let sample = if channels == 1 {
            (left + right) * 0.5
        } else if channel == 0 {
            left
        } else if channel == 1 {
            right
        } else {
            0.0
        };

        sample * 0.65
    }

    let stream = match sample_format {
        SampleFormat::F32 => {
            let pcm = Arc::clone(&pcm);
            let mut position = 0_usize;

            device.build_output_stream(
                config.clone(),
                move |data: &mut [f32], _| {
                    for output in data {
                        *output = sample_at(&pcm, position, channels).clamp(-1.0, 1.0);

                        position += 1;
                    }
                },
                |error| eprintln!("CPAL error: {error}"),
                None,
            )
        }

        SampleFormat::I16 => {
            let pcm = Arc::clone(&pcm);
            let mut position = 0_usize;

            device.build_output_stream(
                config.clone(),
                move |data: &mut [i16], _| {
                    for output in data {
                        let sample = sample_at(&pcm, position, channels).clamp(-1.0, 1.0);

                        *output = (sample * i16::MAX as f32) as i16;

                        position += 1;
                    }
                },
                |error| eprintln!("CPAL error: {error}"),
                None,
            )
        }

        SampleFormat::U16 => {
            let pcm = Arc::clone(&pcm);
            let mut position = 0_usize;

            device.build_output_stream(
                config.clone(),
                move |data: &mut [u16], _| {
                    for output in data {
                        let sample = sample_at(&pcm, position, channels).clamp(-1.0, 1.0);

                        *output = ((sample + 1.0) * 0.5 * u16::MAX as f32) as u16;

                        position += 1;
                    }
                },
                |error| eprintln!("CPAL error: {error}"),
                None,
            )
        }

        other => {
            return Err(format!("Unsupported output sample format: {other:?}"));
        }
    }
    .map_err(|e| e.to_string())?;

    stream.play().map_err(|e| e.to_string())?;
    std::thread::sleep(Duration::from_secs(5));

    Ok("Native music preview completed!".into())
}
