use ffmpeg_next as ffmpeg;

use base64::{Engine as _, engine::general_purpose};

pub fn extract_artwork(path: &str) -> Result<Option<String>, String> {
    ffmpeg::init().map_err(|e| e.to_string())?;

    let mut input = ffmpeg::format::input(path).map_err(|e| e.to_string())?;

    for (stream, packet) in input.packets() {
        // Embedded cover artwork is commonly stored as
        // an attached-picture video stream.
        if stream.parameters().medium() != ffmpeg::media::Type::Video {
            continue;
        }

        let Some(data) = packet.data() else {
            continue;
        };

        if data.is_empty() {
            continue;
        }

        // Common attached cover formats.
        let mime = if data.starts_with(&[0xFF, 0xD8, 0xFF]) {
            "image/jpeg"
        } else if data.starts_with(b"\x89PNG\r\n\x1A\n") {
            "image/png"
        } else if data.starts_with(b"RIFF") && data.get(8..12) == Some(b"WEBP") {
            "image/webp"
        } else {
            continue;
        };

        let encoded = general_purpose::STANDARD.encode(data);

        return Ok(Some(format!("data:{mime};base64,{encoded}")));
    }

    Ok(None)
}
