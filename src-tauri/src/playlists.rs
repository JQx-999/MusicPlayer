use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf, sync::Mutex};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Playlist {
    pub id: String,
    pub name: String,
    pub tracks: Vec<String>,
}

pub struct PlaylistStore {
    file: PathBuf,
    playlists: Mutex<Vec<Playlist>>,
}

impl PlaylistStore {
    pub fn new(file: PathBuf) -> Result<Self, String> {
        let playlists = if file.exists() {
            let contents = fs::read_to_string(&file).map_err(|e| e.to_string())?;

            serde_json::from_str(&contents).map_err(|e| e.to_string())?
        } else {
            Vec::new()
        };

        Ok(Self {
            file,
            playlists: Mutex::new(playlists),
        })
    }

    fn save(&self, playlists: &[Playlist]) -> Result<(), String> {
        if let Some(parent) = self.file.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }

        let contents = serde_json::to_string_pretty(playlists).map_err(|e| e.to_string())?;

        let temporary = self.file.with_extension("json.tmp");

        fs::write(&temporary, contents).map_err(|e| e.to_string())?;

        fs::rename(&temporary, &self.file).map_err(|e| e.to_string())
    }

    pub fn list(&self) -> Result<Vec<Playlist>, String> {
        Ok(self.playlists.lock().map_err(|e| e.to_string())?.clone())
    }

    pub fn create(&self, name: String) -> Result<Playlist, String> {
        let name = name.trim();

        if name.is_empty() {
            return Err("Playlist name cannot be empty".into());
        }

        let mut playlists = self.playlists.lock().map_err(|e| e.to_string())?;

        let playlist = Playlist {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.to_string(),
            tracks: Vec::new(),
        };

        let mut updated = playlists.clone();
        updated.push(playlist.clone());

        self.save(&updated)?;
        *playlists = updated;

        Ok(playlist)
    }

    pub fn add_track(&self, id: &str, path: String) -> Result<(), String> {
        if !std::path::Path::new(&path).is_file() {
            return Err("Track file does not exist".into());
        }

        let mut playlists = self.playlists.lock().map_err(|e| e.to_string())?;

        let mut updated = playlists.clone();

        let playlist = updated
            .iter_mut()
            .find(|p| p.id == id)
            .ok_or("Playlist not found")?;

        if !playlist.tracks.contains(&path) {
            playlist.tracks.push(path);
        }

        self.save(&updated)?;
        *playlists = updated;

        Ok(())
    }

    pub fn remove_track(&self, id: &str, path: &str) -> Result<(), String> {
        let mut playlists = self.playlists.lock().map_err(|e| e.to_string())?;

        let mut updated = playlists.clone();

        let playlist = updated
            .iter_mut()
            .find(|p| p.id == id)
            .ok_or("Playlist not found")?;

        playlist.tracks.retain(|item| item != path);

        self.save(&updated)?;
        *playlists = updated;

        Ok(())
    }

    pub fn delete(&self, id: &str) -> Result<(), String> {
        let mut playlists = self.playlists.lock().map_err(|e| e.to_string())?;

        if !playlists.iter().any(|p| p.id == id) {
            return Err("Playlist not found".into());
        }

        let updated: Vec<_> = playlists.iter().filter(|p| p.id != id).cloned().collect();

        self.save(&updated)?;
        *playlists = updated;

        Ok(())
    }
}
