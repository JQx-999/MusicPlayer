use std::{
    path::{Path, PathBuf},
    sync::Mutex,
};

use tauri::{AppHandle, Emitter, Manager, State};

#[derive(Default)]
pub struct OpenWithState {
    pub pending: Mutex<Vec<String>>,
}

fn is_audio(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase())
            .as_deref(),
        Some("mp3" | "flac" | "wav" | "ogg" | "opus" | "m4a" | "aac" | "aiff" | "aif" | "wma")
    )
}

pub fn queue_files(app: &AppHandle, args: &[String], cwd: &Path) {
    let mut incoming = Vec::new();

    for arg in args.iter().skip(1) {
        let path = PathBuf::from(arg);

        let absolute = if path.is_absolute() {
            path
        } else {
            cwd.join(path)
        };

        if !absolute.is_file() || !is_audio(&absolute) {
            continue;
        }

        if let Ok(canonical) = absolute.canonicalize() {
            incoming.push(canonical.to_string_lossy().into_owned());
        }
    }

    if incoming.is_empty() {
        return;
    }

    let state = app.state::<OpenWithState>();

    {
        let mut pending = state.pending.lock().unwrap();

        for path in incoming {
            if !pending.contains(&path) {
                pending.push(path);
            }
        }
    }

    let _ = app.emit("musicplayer://open-with", ());
}

#[tauri::command]
pub fn take_open_with_files(state: State<'_, OpenWithState>) -> Vec<String> {
    std::mem::take(&mut *state.pending.lock().unwrap())
}
