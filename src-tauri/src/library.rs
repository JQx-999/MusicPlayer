use nucleo_matcher::{
    Config, Matcher, Utf32Str,
    pattern::{CaseMatching, Normalization, Pattern},
};
use serde::Serialize;
use std::{path::Path, sync::Mutex};
use walkdir::WalkDir;

const EXTENSIONS: &[&str] = &[
    "mp3", "flac", "wav", "ogg", "opus", "m4a", "aac", "aiff", "ape", "wma",
];

#[derive(Debug, Clone, Serialize)]
pub struct LibraryTrack {
    pub path: String,
    pub title: String,
}

#[derive(Default)]
pub struct MusicLibrary {
    tracks: Mutex<Vec<LibraryTrack>>,
}

impl MusicLibrary {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn scan(&self, folder: &str) -> Result<usize, String> {
        let root = Path::new(folder);

        if !root.is_dir() {
            return Err("Selected path is not a directory".into());
        }

        let mut found = Vec::new();

        for entry in WalkDir::new(root)
            .follow_links(false)
            .into_iter()
            .filter_map(Result::ok)
        {
            if !entry.file_type().is_file() {
                continue;
            }

            let path = entry.path();

            let supported = path
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| {
                    EXTENSIONS
                        .iter()
                        .any(|allowed| ext.eq_ignore_ascii_case(allowed))
                })
                .unwrap_or(false);

            if !supported {
                continue;
            }

            let title = path
                .file_stem()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();

            found.push(LibraryTrack {
                path: path.to_string_lossy().into_owned(),
                title,
            });
        }

        found.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));

        let count = found.len();

        *self.tracks.lock().map_err(|e| e.to_string())? = found;

        Ok(count)
    }

    pub fn search(&self, query: &str) -> Result<Vec<LibraryTrack>, String> {
        let tracks = self.tracks.lock().map_err(|e| e.to_string())?;

        if query.trim().is_empty() {
            return Ok(tracks.iter().take(100).cloned().collect());
        }

        let mut matcher = Matcher::new(Config::DEFAULT);

        let pattern = Pattern::parse(query.trim(), CaseMatching::Ignore, Normalization::Smart);

        let mut matches = Vec::new();
        let mut buffer = Vec::new();

        for track in tracks.iter() {
            let haystack = Utf32Str::new(&track.title, &mut buffer);

            if let Some(score) = pattern.score(haystack, &mut matcher) {
                matches.push((score, track));
            }
        }

        matches.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.title.cmp(&b.1.title)));

        Ok(matches
            .into_iter()
            .take(100)
            .map(|(_, track)| track.clone())
            .collect())
    }
}
