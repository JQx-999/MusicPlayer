use ffmpeg_next as ffmpeg;

#[derive(serde::Serialize)]
pub struct TrackInfo {
    pub duration: f64,
    pub duration_formatted: String,
}

pub fn read_track(path: &str) -> Result<TrackInfo, String> {
    ffmpeg::init().map_err(|e| e.to_string())?;

    let context = ffmpeg::format::input(path).map_err(|e| e.to_string())?;

    let _audio_stream = context
        .streams()
        .best(ffmpeg::media::Type::Audio)
        .ok_or("No audio stream found")?;

    let raw_duration = context.duration();

    let duration = if raw_duration >= 0 {
        raw_duration as f64 / f64::from(ffmpeg::ffi::AV_TIME_BASE)
    } else {
        0.0
    };

    let minutes = (duration / 60.0).floor() as u64;
    let seconds = (duration % 60.0).floor() as u64;

    Ok(TrackInfo {
        duration,
        duration_formatted: format!("{minutes}:{seconds:02}"),
    })
}

#[derive(serde::Serialize)]
pub struct DecodeInfo {
    pub sample_rate: u32,
    pub channels: u16,
    pub decoded_frames: usize,
    pub decoded_seconds: f64,
}

pub fn test_decode(path: &str) -> Result<DecodeInfo, String> {
    ffmpeg::init().map_err(|e| e.to_string())?;

    let mut input = ffmpeg::format::input(path).map_err(|e| e.to_string())?;
    let stream = input
        .streams()
        .best(ffmpeg::media::Type::Audio)
        .ok_or("No audio stream found")?;

    let stream_index = stream.index();
    let parameters = stream.parameters();

    let context =
        ffmpeg::codec::context::Context::from_parameters(parameters).map_err(|e| e.to_string())?;

    let mut decoder = context.decoder().audio().map_err(|e| e.to_string())?;

    let sample_rate = decoder.rate();
    let channels = decoder.channels();

    let mut decoded_frames = 0_usize;
    let mut decoded = ffmpeg::frame::Audio::empty();

    for (packet_stream, packet) in input.packets() {
        if packet_stream.index() != stream_index {
            continue;
        }

        decoder.send_packet(&packet).map_err(|e| e.to_string())?;
        while decoder.receive_frame(&mut decoded).is_ok() {
            decoded_frames += decoded.samples();
        }
    }
    decoder.send_eof().map_err(|e| e.to_string())?;

    while decoder.receive_frame(&mut decoded).is_ok() {
        decoded_frames += decoded.samples();
    }

    let decoded_seconds = if sample_rate > 0 {
        decoded_frames as f64 / sample_rate as f64
    } else {
        0.0
    };

    Ok(DecodeInfo {
        sample_rate,
        channels,
        decoded_frames,
        decoded_seconds,
    })
}
