#[cfg(target_os = "linux")]
use mpris_server::{Metadata, PlaybackStatus, Player, Time};

#[cfg(target_os = "linux")]
use serde::Deserialize;

#[cfg(target_os = "linux")]
use tauri::{AppHandle, Emitter, State};

#[cfg(target_os = "linux")]
use tokio::sync::mpsc;

#[cfg(target_os = "linux")]
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackMetadata {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub art_url: Option<String>,
    pub duration: f64,
}

#[cfg(target_os = "linux")]
enum MprisMessage {
    Metadata(TrackMetadata),
    Status(PlaybackStatus),
}

#[cfg(target_os = "linux")]
pub struct MprisHandle {
    tx: mpsc::UnboundedSender<MprisMessage>,
}

#[cfg(target_os = "linux")]
#[tauri::command]
pub fn update_mpris_metadata(
    mpris: State<'_, MprisHandle>,
    metadata: TrackMetadata,
) -> Result<(), String> {
    mpris
        .tx
        .send(MprisMessage::Metadata(metadata))
        .map_err(|_| "MPRIS service is unavailable".to_string())
}

#[cfg(target_os = "linux")]
#[tauri::command]
pub fn update_mpris_status(mpris: State<'_, MprisHandle>, status: String) -> Result<(), String> {
    let status = match status.as_str() {
        "playing" => PlaybackStatus::Playing,
        "paused" => PlaybackStatus::Paused,
        "stopped" => PlaybackStatus::Stopped,
        _ => return Err("Invalid MPRIS playback status".into()),
    };

    mpris
        .tx
        .send(MprisMessage::Status(status))
        .map_err(|_| "MPRIS service is unavailable".to_string())
}

#[cfg(target_os = "linux")]
pub fn start(app: AppHandle) -> MprisHandle {
    let (tx, rx) = mpsc::unbounded_channel();

    std::thread::Builder::new()
        .name("resonance-mpris".into())
        .spawn(move || {
            let runtime = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime,
                Err(error) => {
                    eprintln!("[MPRIS] Runtime error: {error}");
                    return;
                }
            };

            let local = tokio::task::LocalSet::new();

            local.block_on(&runtime, async move {
                if let Err(error) = run(app, rx).await {
                    eprintln!("[MPRIS] {error}");
                }
            });
        })
        .expect("Could not start MPRIS thread");

    MprisHandle { tx }
}

#[cfg(target_os = "linux")]
async fn run(app: AppHandle, mut rx: mpsc::UnboundedReceiver<MprisMessage>) -> Result<(), String> {
    let player = Player::builder("resonance")
        .identity("Resonance")
        .can_play(true)
        .can_pause(true)
        .can_control(true)
        .can_go_next(true)
        .can_go_previous(true)
        .build()
        .await
        .map_err(|e| e.to_string())?;

    let toggle_app = app.clone();
    player.connect_play_pause(move |_| {
        let _ = toggle_app.emit("resonance://mpris-play-pause", ());
    });

    let play_app = app.clone();
    player.connect_play(move |_| {
        let _ = play_app.emit("resonance://mpris-play", ());
    });

    let pause_app = app.clone();
    player.connect_pause(move |_| {
        let _ = pause_app.emit("resonance://mpris-pause", ());
    });

    let stop_app = app.clone();
    player.connect_stop(move |_| {
        let _ = stop_app.emit("resonance://mpris-stop", ());
    });

    let next_app = app.clone();

    player.connect_next(move |_| {
        eprintln!("[MPRIS] Next requested");
        let _ = next_app.emit("resonance://mpris-next", ());
    });

    let previous_app = app.clone();

    player.connect_previous(move |_| {
        eprintln!("[MPRIS] Previous requested");
        let _ = previous_app.emit("resonance://mpris-previous", ());
    });

    let server = player.run();
    tokio::pin!(server);

    loop {
        tokio::select! {
            _ = &mut server => break,

            message = rx.recv() => {
                let Some(message) = message else {
                    break;
                };

                match message {
                    MprisMessage::Metadata(track) => {
                        eprintln!("[MPRIS] Metadata update received");
                        eprintln!("[MPRIS] Title: {}", track.title);
                        eprintln!("[MPRIS] Artwork input: {:?}", track.art_url);

                        let mut builder = Metadata::builder()
                            .title(track.title)
                            .artist([track.artist])
                            .album(track.album);

                        if track.duration.is_finite() && track.duration > 0.0 {
                            builder = builder.length(Time::from_micros(
                                (track.duration * 1_000_000.0) as i64,
                            ));
                        }

                        if let Some(url) = track.art_url {
                            if url.starts_with("file://") {
                                eprintln!("[MPRIS] Adding artwork URI: {url}");
                                builder = builder.art_url(url);
                            } else {
                                eprintln!("[MPRIS] Rejected non-file artwork URI");
                            }
                        } else {
                            eprintln!("[MPRIS] No artwork URI received");
                        }

                        match player.set_metadata(builder.build()).await {
                            Ok(()) => eprintln!("[MPRIS] Metadata published successfully"),
                            Err(error) => eprintln!("[MPRIS] Metadata error: {error}"),
                        }
                    }

                    MprisMessage::Status(status) => {
                        if let Err(error) =
                            player.set_playback_status(status).await
                        {
                            eprintln!("[MPRIS] Status error: {error}");
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

#[cfg(target_os = "linux")]
#[tauri::command]
pub fn cache_mpris_artwork(app: tauri::AppHandle, image_data: String) -> Result<String, String> {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    use std::fs;
    use tauri::Manager;

    let (header, encoded) = image_data
        .split_once(',')
        .ok_or("Invalid artwork data URL")?;

    let extension = match header {
        "data:image/jpeg;base64" => "jpg",
        "data:image/png;base64" => "png",
        "data:image/webp;base64" => "webp",
        _ => return Err("Unsupported artwork format".into()),
    };

    if encoded.len() > 14_000_000 {
        return Err("Artwork is too large".into());
    }

    let bytes = STANDARD
        .decode(encoded)
        .map_err(|e| format!("Invalid Base64 artwork: {e}"))?;

    if bytes.len() > 10_000_000 {
        return Err("Decoded artwork is too large".into());
    }

    let cache_dir = app
        .path()
        .app_cache_dir()
        .map_err(|e| e.to_string())?
        .join("mpris-artwork");

    fs::create_dir_all(&cache_dir).map_err(|e| e.to_string())?;

    let filename = format!("{}.{}", uuid::Uuid::new_v4(), extension);

    let image_path = cache_dir.join(filename);

    fs::write(&image_path, bytes).map_err(|e| e.to_string())?;

    let uri =
        tauri::Url::from_file_path(&image_path).map_err(|_| "Could not create artwork file URI")?;

    Ok(uri.to_string())
}
