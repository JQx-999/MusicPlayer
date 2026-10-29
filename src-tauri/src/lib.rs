// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
use audio::player::{AudioPlayer, PlayerState};
mod audio;
mod library;
mod mpris;
#[cfg(target_os = "linux")]
mod open_with;
mod radio;

use audio::radio_player::RadioPlayer;
mod playlists;

use playlists::{Playlist, PlaylistStore};
use tauri::Manager;

use library::{LibraryTrack, MusicLibrary};
use std::sync::Arc;

#[tauri::command]
async fn start_radio(radio: tauri::State<'_, Arc<RadioPlayer>>, url: String) -> Result<(), String> {
    let radio = radio.inner().clone();

    tauri::async_runtime::spawn_blocking(move || radio.start(&url))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn search_radio_stations(query: String) -> Result<Vec<radio::RadioStation>, String> {
    radio::search_stations(query).await
}
#[tauri::command]
async fn stop_radio(radio: tauri::State<'_, Arc<RadioPlayer>>) -> Result<(), String> {
    let radio = radio.inner().clone();

    tauri::async_runtime::spawn_blocking(move || radio.stop())
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
fn get_radio_state(radio: tauri::State<'_, Arc<RadioPlayer>>) -> bool {
    radio.is_playing()
}

#[tauri::command]
fn list_playlists(store: tauri::State<'_, PlaylistStore>) -> Result<Vec<Playlist>, String> {
    store.list()
}

#[tauri::command]
fn create_playlist(
    store: tauri::State<'_, PlaylistStore>,
    name: String,
) -> Result<Playlist, String> {
    store.create(name)
}

#[tauri::command]
fn add_playlist_track(
    store: tauri::State<'_, PlaylistStore>,
    id: String,
    path: String,
) -> Result<(), String> {
    store.add_track(&id, path)
}

#[tauri::command]
async fn get_track_metadata(path: String) -> Result<audio::metadata::TrackMetadata, String> {
    tauri::async_runtime::spawn_blocking(move || audio::metadata::extract_metadata(&path))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
fn remove_playlist_track(
    store: tauri::State<'_, PlaylistStore>,
    id: String,
    path: String,
) -> Result<(), String> {
    store.remove_track(&id, &path)
}

#[tauri::command]
fn delete_playlist(store: tauri::State<'_, PlaylistStore>, id: String) -> Result<(), String> {
    store.delete(&id)
}

#[tauri::command]
async fn scan_music_directory(
    library: tauri::State<'_, std::sync::Arc<MusicLibrary>>,
    folder: String,
) -> Result<usize, String> {
    let library = library.inner().clone();

    tauri::async_runtime::spawn_blocking(move || library.scan(&folder))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn search_music(
    library: tauri::State<'_, std::sync::Arc<MusicLibrary>>,
    query: String,
) -> Result<Vec<LibraryTrack>, String> {
    let library = library.inner().clone();

    tauri::async_runtime::spawn_blocking(move || library.search(&query))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn get_album_art(path: String) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || audio::artwork::extract_artwork(&path))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn load_track(
    player: tauri::State<'_, std::sync::Arc<AudioPlayer>>,
    path: String,
) -> Result<(), String> {
    let player = player.inner().clone();
    tauri::async_runtime::spawn_blocking(move || player.load(&path))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
fn play_track(player: tauri::State<'_, std::sync::Arc<AudioPlayer>>) -> Result<(), String> {
    player.play()
}

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}!", name)
}
// Example inside your Tauri command definitions
#[tauri::command]
fn seek_track(seconds: f64, player: tauri::State<'_, Arc<AudioPlayer>>) -> Result<(), String> {
    player.seek(seconds)
}
#[tauri::command]
fn get_player_state(
    player: tauri::State<'_, std::sync::Arc<AudioPlayer>>,
) -> Result<PlayerState, String> {
    player.get_state()
}

#[tauri::command]
fn pause_track(player: tauri::State<'_, std::sync::Arc<AudioPlayer>>) -> Result<(), String> {
    player.pause()
}

#[tauri::command]
fn resume_track(player: tauri::State<'_, std::sync::Arc<AudioPlayer>>) -> Result<(), String> {
    player.resume()
}

#[tauri::command]
fn stop_track(player: tauri::State<'_, std::sync::Arc<AudioPlayer>>) -> Result<(), String> {
    player.stop()
}

#[tauri::command]
fn set_player_volume(
    player: tauri::State<'_, std::sync::Arc<AudioPlayer>>,
    volume: f32,
) -> Result<(), String> {
    player.set_volume(volume)
}

#[tauri::command]
fn audio_engine_status() -> String {
    String::from("musicplayer is active!")
}
#[tauri::command]
fn test_audio_output() -> Result<String, String> {
    audio::output::test_audio_output()
}
#[tauri::command]
async fn play_preview(path: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || audio::preview::play_preview(&path))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn test_decode(path: String) -> Result<audio::decoder::DecodeInfo, String> {
    tauri::async_runtime::spawn_blocking(move || audio::decoder::test_decode(&path))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
fn get_track_info(path: String) -> Result<audio::decoder::TrackInfo, String> {
    audio::decoder::read_track(&path)
}

#[tauri::command]
async fn gapless_queue(
    player: tauri::State<'_, std::sync::Arc<audio::player::AudioPlayer>>,
    path: String,
) -> Result<(), String> {
    let player = player.inner().clone();

    tauri::async_runtime::spawn_blocking(move || player.gapless_queue(&path))
        .await
        .map_err(|e| e.to_string())?
}
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let data_dir = app.path().app_data_dir().map_err(std::io::Error::other)?;

            let store = PlaylistStore::new(data_dir.join("playlists.json"))
                .map_err(std::io::Error::other)?;

            app.manage(store);
            #[cfg(target_os = "linux")]
            app.manage(mpris::start(app.handle().clone()));

            #[cfg(target_os = "linux")]
            {
                app.manage(open_with::OpenWithState::default());

                let args: Vec<String> = std::env::args().collect();
                let cwd = std::env::current_dir()?;

                open_with::queue_files(app.handle(), &args, &cwd);
            }

            Ok(())
        })
        .manage(Arc::new(RadioPlayer::new()))
        .manage(std::sync::Arc::new(AudioPlayer::new()))
        .manage(Arc::new(MusicLibrary::new()))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_single_instance::init(|app, args, cwd| {
            open_with::queue_files(app, &args, std::path::Path::new(&cwd));

            use tauri::Manager;

            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .invoke_handler(tauri::generate_handler![
            greet,
            audio_engine_status,
            get_track_info,
            test_audio_output,
            test_decode,
            play_preview,
            get_player_state,
            pause_track,
            resume_track,
            stop_track,
            search_radio_stations,
            set_player_volume,
            play_track,
            load_track,
            seek_track,
            get_album_art,
            scan_music_directory,
            search_music,
            list_playlists,
            start_radio,
            stop_radio,
            get_radio_state,
            gapless_queue,
            create_playlist,
            add_playlist_track,
            remove_playlist_track,
            delete_playlist,
            get_track_metadata,
            #[cfg(target_os = "linux")]
            mpris::update_mpris_metadata,
            #[cfg(target_os = "linux")]
            mpris::update_mpris_status,
            #[cfg(target_os = "linux")]
            mpris::update_mpris_status,
            #[cfg(target_os = "linux")]
            mpris::cache_mpris_artwork,
            #[cfg(target_os = "linux")]
            open_with::take_open_with_files
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
