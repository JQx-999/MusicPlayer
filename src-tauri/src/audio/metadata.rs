use ffmpeg_next as ffmpeg;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct TrackMetadata {
    pub title: String,
    pub artist: String,
    pub album: String,
}

pub fn extract_metadata(path: &str) -> Result<TrackMetadata, String> {
    ffmpeg::init().map_err(|e| e.to_string())?;

    let input = ffmpeg::format::input(path).map_err(|e| e.to_string())?;

    let tags = input.metadata();

    let filename = std::path::Path::new(path)
        .file_stem()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Unknown Track".to_string());

    let get_tag = |key: &str| {
        tags.get(key)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    };

    Ok(TrackMetadata {
        title: get_tag("title").unwrap_or(filename),
        artist: get_tag("artist")
            .or_else(|| get_tag("album_artist"))
            .unwrap_or_else(|| "Unknown Artist".to_string()),
        album: get_tag("album").unwrap_or_else(|| "Unknown Album".to_string()),
    })
}
