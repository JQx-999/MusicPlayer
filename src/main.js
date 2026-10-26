import {
  getRecents,
  addRecent,
  clearRecents,
  getRecentArtwork,
  migrateRecentArtwork
} from "./recents.js";

import { initRadio } from "./radio.js";
import { initCollections } from "./collections.js";
import { initMusicSearch } from "./search.js";

const invoke = window.__TAURI__.core.invoke;

const playBtn = document.querySelector("#play-btn");
const nowPlayBtn = document.querySelector("#now-play-btn");
const playImage = document.getElementById("play-img");
const nowPlayImage = document.getElementById("now-play-img");
const volumeSlider = document.querySelector("#volume-slider");
const nowVolumeSlider = document.querySelector("#now-volume-slider");
const progressSlider = document.querySelector("#progress-slider");
const nowProgressSlider = document.querySelector("#now-progress-slider");
const currentTime = document.querySelector("#current-time");
const nowCurrentTime = document.querySelector("#now-current-time");
const totalTime = document.querySelector("#total-time");
const nowTotalTime = document.querySelector("#now-total-time");
const muteBtn = document.querySelector("#mute-btn");
const nowMuteBtn = document.querySelector("#now-mute-btn");
const audioImg = document.getElementById("audio-img");
const nowAudioImg = document.getElementById("now-audio-img");
const thumbnail = document.querySelector(".music-thumbnail")

thumbnail.getAnimations().forEach(animation => animation.pause());

const playlist = [];

let currentTrackIndex = -1;
let trackLoaded = false;
let seeking = false;

let autoAdvancing = false;
let playbackTransitioning = false;
let previousPlaybackStatus = null;
let playerPollBusy = false;

let lastVolume = 1;
let isMuted = false;

let radioActive = false;
let radioPaused = false;
let radioBusy = false;
let currentStation = null;

// --------------------------------------------------
// AUDIO ENGINE TEST
// --------------------------------------------------

async function testEngine() {
  try {
    const response = await invoke("audio_engine_status");
    console.log("[musicplayer] Audio engine:", response);
  } catch (error) {
    console.error("[musicplayer] Audio engine error:", error);
  }
}

testEngine();

// --------------------------------------------------
// TIME FORMATTING
// --------------------------------------------------

function formatTime(seconds) {
  if (!Number.isFinite(seconds)) return "0:00";

  const minutes = Math.floor(seconds / 60);
  const remaining = Math.floor(seconds % 60);

  return `${minutes}:${String(remaining).padStart(2, "0")}`;
}

// --------------------------------------------------
// CONTINUOUS PLAYBACK
// --------------------------------------------------

async function checkTrackFinished(state) {
  if (
    !trackLoaded ||
    autoAdvancing ||
    playbackTransitioning ||
    currentStation ||
    playlist.length === 0
  ) {
    return false;
  }

  const reachedEnd =
    state.duration > 0 &&
    state.position >= state.duration - 0.15;

  const justFinished =
    previousPlaybackStatus === "playing" &&
    state.status === "stopped" &&
    reachedEnd;

  if (!justFinished) return false;

  autoAdvancing = true;

  try {
    const nextIndex = currentTrackIndex + 1;

    if (nextIndex < playlist.length) {
      console.log(
        `[musicplayer] Auto-playing ${nextIndex + 1}/${playlist.length}`
      );

      await loadSong(nextIndex);
    } else {
      console.log("[musicplayer] Playlist finished.");

      trackLoaded = false;
      previousPlaybackStatus = "stopped";

      playBtn?.classList.remove("is-playing");

      if (playImage) {
        playImage.src =
          "assets/media-playback-start-symbolic.svg";
      }
      if (nowPlayImage) {
        nowPlayImage.src =
          "assets/media-playback-start-symbolic.svg";
      }

    }
  } catch (error) {
    console.error("[musicplayer] Auto-next failed:", error);
  } finally {
    autoAdvancing = false;
  }

  return true;
}

// --------------------------------------------------
// PLAYER UI
// --------------------------------------------------

async function updatePlayerUI() {
  if (currentStation || playbackTransitioning) return;

  const state = await invoke("get_player_state");

  if (currentStation || playbackTransitioning) return;

  playBtn?.classList.toggle(
    "is-playing",
    state.status === "playing"
  );
  nowPlayBtn?.classList.toggle(
    "is-playing",
    state.status === "playing"
  );

  if (playImage) {
    playImage.src =
      state.status === "playing"
        ? "assets/media-playback-pause-symbolic.svg"
        : "assets/media-playback-start-symbolic.svg";
  }
  if (nowPlayImage) {
    nowPlayImage.src =
      state.status === "playing"
        ? "assets/media-playback-pause-symbolic.svg"
        : "assets/media-playback-start-symbolic.svg";
  }

  if (currentTime) {
    currentTime.textContent = formatTime(state.position);
  }

  if (nowCurrentTime) {
    nowCurrentTime.textContent = formatTime(state.position);
  }

  if (totalTime) {
    totalTime.textContent = formatTime(state.duration);
  }

  if (nowTotalTime) {
    nowTotalTime.textContent = formatTime(state.duration);
  }

  if (progressSlider && !seeking) {
    progressSlider.max = state.duration || 1;
    progressSlider.value = state.position;
  }

  if (nowProgressSlider && !seeking) {
    nowProgressSlider.max = state.duration || 1;
    nowProgressSlider.value = state.position;
  }

  const advanced = await checkTrackFinished(state);

  if (!advanced && !playbackTransitioning && !currentStation) {
    previousPlaybackStatus = state.status;
  }
}

setInterval(async () => {
  if (
    !trackLoaded ||
    playerPollBusy ||
    playbackTransitioning ||
    currentStation
  ) {
    return;
  }

  playerPollBusy = true;

  try {
    await updatePlayerUI();
  } catch (error) {
    console.error("[musicplayer] Player polling failed:", error);
  } finally {
    playerPollBusy = false;
  }
}, 250);

// --------------------------------------------------
// LOAD SONG
// --------------------------------------------------


async function loadSong(index) {
  if (
    index < 0 ||
    index >= playlist.length ||
    playbackTransitioning
  ) {
    return false;
  }

  playbackTransitioning = true;

  const path = playlist[index];

  try {
    if (currentStation) {
      await invoke("stop_radio");

      radioActive = false;
      radioPaused = false;
      currentStation = null;

      updateRadioControls();
    }

    await invoke("load_track", { path });

    currentTrackIndex = index;
    trackLoaded = true;

    await invoke("play_track");

    previousPlaybackStatus = "playing";

    thumbnail
      .getAnimations()
      .forEach(animation => animation.play());

    if (playImage) {
      playImage.src =
        "assets/media-playback-pause-symbolic.svg";
    }

    playBtn?.classList.add("is-playing");

    let artwork = null;

    let metadata = {
      title: path
        .split(/[\\/]/)
        .pop()
        .replace(/\.[^.]+$/, ""),
      artist: "Unknown Artist",
      album: "Unknown Album"
    };

    const [artResult, metadataResult] =
      await Promise.allSettled([
        invoke("get_album_art", { path }),
        invoke("get_track_metadata", { path })
      ]);

    if (artResult.status === "fulfilled") {
      artwork = artResult.value;
    } else {
      console.warn(
        "[musicplayer] Artwork extraction failed:",
        artResult.reason
      );
    }
    if (metadataResult.status === "fulfilled") {
      const extracted = metadataResult.value;

      const validText = value =>
        typeof value === "string" &&
        value.trim().length > 0;

      metadata = {
        title: validText(extracted?.title)
          ? extracted.title.trim()
          : metadata.title,

        artist: validText(extracted?.artist)
          ? extracted.artist.trim()
          : metadata.artist,

        album: validText(extracted?.album)
          ? extracted.album.trim()
          : metadata.album
      };
    } else {
      console.warn(
        "[musicplayer] Metadata extraction failed:",
        metadataResult.reason
      );
    }

    console.log(
      "[musicplayer] Final track metadata:",
      metadata
    );

    let mprisArtUrl = null;

    if (
      typeof artwork === "string" &&
      artwork.startsWith("data:image/")
    ) {
      try {
        mprisArtUrl = await invoke(
          "cache_mpris_artwork",
          {
            imageData: artwork
          }
        );

        console.log(
          "[MPRIS] Cached artwork URI:",
          mprisArtUrl
        );
      } catch (error) {
        console.warn(
          "[MPRIS] Could not cache album artwork:",
          error
        );
      }
    }

    try {
      const state = await invoke("get_player_state");

      await invoke("update_mpris_metadata", {
        metadata: {
          title: metadata.title,
          artist: metadata.artist,
          album: metadata.album,
          artUrl: mprisArtUrl,
          duration: state.duration || 0
        }
      });

      await invoke("update_mpris_status", {
        status: "playing"
      });

      console.log(
        "[MPRIS] Published track:",
        metadata.title,
        metadata.artist,
        metadata.album
      );
    } catch (error) {
      console.warn(
        "[MPRIS] Media controls update failed:",
        error
      );
    }

    const nowPlayingArtist = document.getElementById("now-playing-artist");
    nowPlayingArtist.textContent = metadata.artist;

    try {
      await addRecent({
        path,
        title: metadata.title,
        artist: metadata.artist,
        album: metadata.album,
        artwork
      });

      await renderRecents();
    } catch (error) {
      console.warn(
        "[musicplayer] Could not update Recents:",
        error
      );
    }

    const nameElement =
      document.querySelector("#music-name");

    const nowNameElement = document.querySelector("#now-playing-name");

    if (nowNameElement) {
      nowNameElement.textContent = metadata.title;
    }

    if (nameElement) {
      nameElement.textContent = metadata.title;
    }

    console.log(
      `[musicplayer] Playing ${currentTrackIndex + 1}/${playlist.length}:`,
      metadata.title
    );

    return true;

  } catch (error) {
    console.error(
      "[musicplayer] Failed to load song:",
      error
    );

    trackLoaded = false;
    previousPlaybackStatus = null;

    return false;

  } finally {
    playbackTransitioning = false;
  }
}


// --------------------------------------------------
// PLAY / PAUSE
// --------------------------------------------------

async function togglePlayback() {
  try {
    if (currentStation) {
      if (radioBusy) return;

      if (radioActive) {
        await pauseRadio();
      } else if (radioPaused) {
        await playRadioStation(currentStation);
      }

      return;
    }

    if (!trackLoaded || playbackTransitioning) {
      console.warn("[musicplayer] Select a song first.");
      return;
    }

    const state = await invoke("get_player_state");

    if (state.status === "playing") {
      await invoke("pause_track");
      thumbnail.getAnimations().forEach(animation => animation.pause());
    } else if (state.status === "paused") {
      await invoke("resume_track");
      thumbnail.getAnimations().forEach(animation => animation.play());
    } else {
      await invoke("play_track");
      thumbnail.getAnimations().forEach(animation => animation.play());
    }

    await updatePlayerUI();
  } catch (error) {
    console.error("[musicplayer] Playback error:", error);
  }
}

playBtn?.addEventListener("click", togglePlayback);
nowPlayBtn?.addEventListener("click", togglePlayback);

// --------------------------------------------------
// KEYBOARD SHORTCUTS
// --------------------------------------------------

window.addEventListener("keydown", event => {
  const target = event.target;
  if (
    target instanceof HTMLElement &&
    (
      target.matches("input, textarea, select") ||
      target.isContentEditable
    )
  ) {
    return;
  }

  if (event.code === "Space" && !event.repeat) {
    event.preventDefault();
    togglePlayback();
  }

  if (event.key.toLowerCase() === "m" && !event.repeat) {
    muteBtn?.click();
    nowMuteBtn?.click();
  }

  if (event.key === "/" && !event.repeat) {
    sidebarToggle?.click();
  }
});

// --------------------------------------------------
// VOLUME
// --------------------------------------------------

volumeSlider?.addEventListener("input", async event => {
  const volume = Number(event.target.value) / 100;

  try {
    await invoke("set_player_volume", { volume });

    if (volume > 0) {
      lastVolume = volume;
      isMuted = false;

      muteBtn?.classList.remove("is-muted");
      nowMuteBtn?.classList.remove("is-muted");

      if (audioImg) {
        audioImg.src =
          "assets/xsi-audio-volume-high-symbolic.svg";
      }
    }
  } catch (error) {
    console.error("[musicplayer] Volume error:", error);
  }
});

nowVolumeSlider?.addEventListener("input", async event => {
  const volume = Number(event.target.value) / 100;

  try {
    await invoke("set_player_volume", { volume });

    if (volume > 0) {
      lastVolume = volume;
      isMuted = false;

      muteBtn?.classList.remove("is-muted");
      nowMuteBtn?.classList.remove("is-muted");

      if (audioImg) {
        audioImg.src =
          "assets/xsi-audio-volume-high-symbolic.svg";
      }
    }
  } catch (error) {
    console.error("[musicplayer] Volume error:", error);
  }
});

muteBtn?.addEventListener("click", async () => {
  try {
    if (!isMuted) {
      const currentVolume = volumeSlider
        ? Number(volumeSlider.value) / 100
        : lastVolume;

      if (currentVolume > 0) {
        lastVolume = currentVolume;
      }

      await invoke("set_player_volume", { volume: 0 });

      isMuted = true;

      if (audioImg) {
        audioImg.src =
          "assets/xsi-audio-volume-muted-symbolic.svg";
      }

      if (volumeSlider) {
        volumeSlider.value = 0;
      }
    } else {
      await invoke("set_player_volume", {
        volume: lastVolume
      });

      isMuted = false;

      if (audioImg) {
        audioImg.src =
          "assets/xsi-audio-volume-high-symbolic.svg";
      }

      if (volumeSlider) {
        volumeSlider.value = lastVolume * 100;
      }
    }

    muteBtn?.classList.toggle("is-muted", isMuted);

    muteBtn?.setAttribute(
      "aria-label",
      isMuted ? "Unmute" : "Mute"
    );
  } catch (error) {
    console.error("[musicplayer] Mute failed:", error);
  }
});

nowMuteBtn?.addEventListener("click", async () => {
  try {
    if (!isMuted) {
      const currentVolume = volumeSlider
        ? Number(volumeSlider.value) / 100
        : lastVolume;

      if (currentVolume > 0) {
        lastVolume = currentVolume;
      }

      await invoke("set_player_volume", { volume: 0 });

      isMuted = true;

      if (nowAudioImg) {
        nowAudioImg.src =
          "assets/xsi-audio-volume-muted-symbolic.svg";
      }

      if (volumeSlider) {
        volumeSlider.value = 0;
      }
    } else {
      await invoke("set_player_volume", {
        volume: lastVolume
      });

      isMuted = false;

      if (nowAudioImg) {
        nowAudioImg.src =
          "assets/xsi-audio-volume-high-symbolic.svg";
      }

      if (volumeSlider) {
        volumeSlider.value = lastVolume * 100;
      }
    }

    nowMuteBtn?.classList.toggle("is-muted", isMuted);

    nowMuteBtn?.setAttribute(
      "aria-label",
      isMuted ? "Unmute" : "Mute"
    );
  } catch (error) {
    console.error("[musicplayer] Mute failed:", error);
  }
});

// --------------------------------------------------
// COLLECTIONS
// --------------------------------------------------

async function playCollection(paths, startIndex = 0) {
  if (!Array.isArray(paths) || paths.length === 0) return;

  if (playbackTransitioning) return;

  playlist.length = 0;
  playlist.push(...paths);

  currentTrackIndex = -1;
  previousPlaybackStatus = null;

  await loadSong(startIndex);
}

initCollections(playCollection);

// --------------------------------------------------
// MUSIC SEARCH
// --------------------------------------------------

initMusicSearch(async track => {
  if (playbackTransitioning) return;

  let index = playlist.indexOf(track.path);

  if (index === -1) {
    playlist.push(track.path);
    index = playlist.length - 1;
  }

  await loadSong(index);
});

// --------------------------------------------------
// OPEN LOCAL MUSIC FILES
// --------------------------------------------------

async function chooseMusic() {
  try {
    const selected = await window.__TAURI__.dialog.open({
      multiple: true,
      directory: false,
      filters: [{
        name: "Music",
        extensions: [
          "mp3",
          "flac",
          "wav",
          "ogg",
          "m4a",
          "aac"
        ]
      }]
    });

    if (!selected) return;

    const paths = Array.isArray(selected)
      ? selected
      : [selected];

    if (paths.length === 0) return;

    const firstNewIndex = playlist.length;

    playlist.push(...paths);

    await loadSong(firstNewIndex);
  } catch (error) {
    console.error("[musicplayer] File picker error:", error);
  }
}

document
  .querySelector("#file-btn")
  ?.addEventListener("click", chooseMusic);

// --------------------------------------------------
// NEXT / PREVIOUS
// --------------------------------------------------

document
  .querySelector("#next-btn")
  ?.addEventListener("click", async () => {
    if (
      playlist.length === 0 ||
      playbackTransitioning
    ) {
      return;
    }

    const next =
      (currentTrackIndex + 1) % playlist.length;

    await loadSong(next);
  });

document
  .querySelector("#now-next-btn")
  ?.addEventListener("click", async () => {
    if (
      playlist.length === 0 ||
      playbackTransitioning
    ) {
      return;
    }

    const next =
      (currentTrackIndex + 1) % playlist.length;

    await loadSong(next);
  });

document
  .querySelector("#now-prev-btn")
  ?.addEventListener("click", async () => {
    if (
      playlist.length === 0 ||
      playbackTransitioning
    ) {
      return;
    }

    const previous =
      (currentTrackIndex - 1 + playlist.length) %
      playlist.length;

    await loadSong(previous);
  });

// --------------------------------------------------
// SEEKING
// --------------------------------------------------

progressSlider?.addEventListener("pointerdown", () => {
  if (!currentStation) {
    seeking = true;
  }
});

progressSlider?.addEventListener("input", event => {
  if (currentStation) return;

  seeking = true;

  if (currentTime) {
    currentTime.textContent =
      formatTime(Number(event.target.value));
  }
});

progressSlider?.addEventListener("change", async event => {
  if (currentStation) {
    seeking = false;
    return;
  }

  const seconds = Number(event.target.value);

  try {
    await invoke("seek_track", { seconds });
    await updatePlayerUI();
  } catch (error) {
    console.error("[musicplayer] Seeking failed:", error);
  } finally {
    seeking = false;
  }
});

nowProgressSlider?.addEventListener("pointerdown", () => {
  if (!currentStation) {
    seeking = true;
  }
});

nowProgressSlider?.addEventListener("input", event => {
  if (currentStation) return;

  seeking = true;

  if (currentTime) {
    currentTime.textContent =
      formatTime(Number(event.target.value));
  }
});

nowProgressSlider?.addEventListener("change", async event => {
  if (currentStation) {
    seeking = false;
    return;
  }

  const seconds = Number(event.target.value);

  try {
    await invoke("seek_track", { seconds });
    await updatePlayerUI();
  } catch (error) {
    console.error("[musicplayer] Seeking failed:", error);
  } finally {
    seeking = false;
  }
});

// --------------------------------------------------
// RECENTLY PLAYED
// --------------------------------------------------

let recentsRenderVersion = 0;

async function renderRecents() {
  const version = ++recentsRenderVersion;

  const container =
    document.querySelector("#recents-list");

  const musicThumbnail =
    document.querySelector(".music-thumbnail");

  const nowPlayingThumbnail = document.querySelector(".now-playing-image");

  if (!container) return;

  const recents = getRecents();

  container.replaceChildren();

  if (nowPlayingThumbnail) {
    nowPlayingThumbnail.replaceChildren();
    if (recents.length > 0) {
      const latestTrack = recents[0];
      nowPlayingThumbnail.textContent = "🎶";
      try {
        const artwork = await getRecentArtwork(latestTrack.path);
        if (version !== recentsRenderVersion) return;
        if (artwork) {
          const thumbImg = document.createElement("img");
          thumbImg.src = artwork;
          thumbImg.className = "now-img";



          thumbImg.addEventListener("error", () => {
            thumbImg.remove();
            nowPlayingThumbnail.textContent = "🎶";
          });


          nowPlayingThumbnail.replaceChildren(thumbImg);
        }
      } catch (error) {
        console.warn(
          "[musicplayer] Failed to load thumbnail:",
          error
        );
      }
    } else {
      nowPlayingThumbnail.textContent = "🎶"
    }
  }

  if (musicThumbnail) {
    musicThumbnail.replaceChildren();

    if (recents.length > 0) {
      const latestTrack = recents[0];

      musicThumbnail.textContent = "🎶";

      try {
        const artwork =
          await getRecentArtwork(latestTrack.path);

        if (version !== recentsRenderVersion) return;

        if (artwork) {
          const thumbImg = document.createElement("img");

          thumbImg.src = artwork;
          thumbImg.className = "thumbnail-img";
          thumbImg.alt = "Current track artwork";

          if (thumbImg) {
            const bgContainer = document.querySelector('.now-playing');
            const canvas = document.createElement('canvas');
            const ctx = canvas.getContext('2d');
            canvas.width = thumbImg.naturalWidth || thumbImg.width;
            canvas.height = thumbImg.naturalHeight || thumbImg.height;
            ctx.drawImage(thumbImg, 0, 0);
            const p1 = ctx.getImageData(canvas.width * 0.2, canvas.height * 0.2, 1, 1).data;
            const p2 = ctx.getImageData(canvas.width * 0.8, canvas.height * 0.5, 1, 1).data;
            const p3 = ctx.getImageData(canvas.width * 0.5, canvas.height * 0.8, 1, 1).data;

            const color1 = `rgba(${p1[0]}, ${p1[1]}, ${p1[2]}, 0.5)`;
            const color2 = `rgba(${p2[0]}, ${p2[1]}, ${p2[2]}, 0.5)`;
            const color3 = `rgba(${p3[0]}, ${p3[1]}, ${p3[2]}, 0.5)`;

            bgContainer.style.setProperty('--color1', color1);
            bgContainer.style.setProperty('--color2', color2);
            bgContainer.style.setProperty('--color3', color3);
          }

          thumbImg.addEventListener("error", () => {
            thumbImg.remove();
            musicThumbnail.textContent = "🎶";
          });

          musicThumbnail.replaceChildren(thumbImg);
        }
      } catch (error) {
        console.warn(
          "[musicplayer] Failed to load thumbnail:",
          error
        );
      }
    } else {
      musicThumbnail.textContent = "🎶";
    }
  }

  if (version !== recentsRenderVersion) return;

  if (recents.length === 0) {
    const empty = document.createElement("p");

    empty.className = "recents-empty";
    empty.textContent =
      "Your recently played songs will appear here.";

    container.appendChild(empty);
    return;
  }

  for (const track of recents) {
    const card = document.createElement("button");

    card.type = "button";
    card.className = "recent-card";

    const art = document.createElement("div");
    art.className = "recent-art";

    art.textContent = "♫";

    const title = document.createElement("span");
    title.className = "recent-title";
    title.textContent = track.title;

    const artist = document.createElement("span");
    artist.className = "recent-artist";
    artist.textContent = track.artist;

    card.append(art, title, artist);

    card.addEventListener("click", async () => {
      try {
        await playRecent(track);
      } catch (error) {
        console.error(
          "[musicplayer] Failed to play Recent:",
          error
        );
      }
    });

    container.appendChild(card);

    getRecentArtwork(track.path)
      .then(artwork => {

        if (
          version !== recentsRenderVersion ||
          !artwork ||
          !card.isConnected
        ) {
          return;
        }

        const image = document.createElement("img");

        image.src = artwork;
        image.alt = `${track.title} album artwork`;
        image.loading = "lazy";

        image.addEventListener("error", () => {
          image.remove();
          art.textContent = "♫";
        });

        art.replaceChildren(image);
      })
      .catch(error => {
        console.warn(
          "[musicplayer] Failed to load RecentnowPlayingArtist(artist.textContent); artwork:",
          error
        );
      });
  }
}

async function playRecent(track) {

  let index = playlist.indexOf(track.path);

  if (index === -1) {
    playlist.push(track.path);
    index = playlist.length - 1;
  }

  await loadSong(index);
}

document
  .querySelector("#clear-recents")
  ?.addEventListener("click", async () => {
    recentsRenderVersion++;

    await clearRecents();
    await renderRecents();
  });

(async () => {
  try {
    await migrateRecentArtwork();
  } catch (error) {
    console.warn(
      "[musicplayer] Recent artwork migration failed:",
      error
    );
  }

  await renderRecents();
})();

// --------------------------------------------------
// PAGE NAVIGATION
// --------------------------------------------------

const homeBtn = document.getElementById("home");
const searchBtn = document.getElementById("search");
const collectionsBtn = document.getElementById("collections");
const streamBtn = document.getElementById("stream");
const optionsBtn = document.getElementById("options")

const sideBtn = document.querySelectorAll(".btn");

const homeBtnQuery = document.querySelector("#home");
const searchBtnQuery = document.querySelector("#search");
const collectionsBtnQuery =
  document.querySelector("#collections");
const streamBtnQuery = document.querySelector("#stream");
const optionsBtnQuery = document.querySelector("#options");

const mainSec = document.querySelector(".main-section");
const homeSec = document.querySelector("#home-section");
const searchSec = document.querySelector("#search-section");
const collectionsSec =
  document.querySelector("#collections-section");
const streamSec = document.querySelector("#stream-section");
const optionsSec = document.querySelector("#options-section");

homeBtn?.addEventListener("click", () => {
  mainSec.style.setProperty("display", "none");
  searchSec.style.setProperty("display", "none");
  collectionsSec.style.setProperty("display", "none");
  streamSec.style.setProperty("display", "none");
  optionsSec.style.setProperty("display", "none");

  homeSec.style.setProperty("display", "block");


  collectionsBtnQuery.style.setProperty(
    "background",
    "rgba(255, 255, 255, 0.02)"
  );

  streamBtnQuery.style.setProperty(
    "background",
    "rgba(255, 255, 255, 0.02)"
  );

  searchBtnQuery.style.setProperty(
    "background",
    "rgba(255, 255, 255, 0.02)"
  );

  homeBtnQuery.style.setProperty(
    "background",
    "rgba(125, 0, 0, 0.75)"
  );

  optionsBtnQuery.style.setProperty(
    "background",
    "rgba(255, 255, 255, 0.02)"
  )
});

searchBtn?.addEventListener("click", () => {
  mainSec.style.setProperty("display", "none");
  collectionsSec.style.setProperty("display", "none");
  streamSec.style.setProperty("display", "none");
  optionsSec.style.setProperty("display", "none");

  searchSec.style.setProperty("display", "block");


  homeBtnQuery.style.setProperty(
    "background",
    "rgba(255, 255, 255, 0.02)"
  );

  searchBtnQuery.style.setProperty(
    "background",
    "rgba(0, 75, 0, 0.75)"
  );

  collectionsBtnQuery.style.setProperty(
    "background",
    "rgba(255, 255, 255, 0.02)"
  );

  streamBtnQuery.style.setProperty(
    "background",
    "rgba(255, 255, 255, 0.02)"
  );

  optionsBtnQuery.style.setProperty(
    "background",
    "rgba(255, 255, 255, 0.02)"
  )
});

collectionsBtn?.addEventListener("click", () => {
  mainSec.style.setProperty("display", "none");
  searchSec.style.setProperty("display", "none");
  streamSec.style.setProperty("display", "none");
  optionsSec.style.setProperty("display", "none");

  collectionsSec.style.setProperty("display", "block");


  homeBtnQuery.style.setProperty(
    "background",
    "rgba(255, 255, 255, 0.02)"
  );

  collectionsBtnQuery.style.setProperty(
    "background",
    "rgba(0, 0, 100, 0.55)"
  );

  searchBtnQuery.style.setProperty(
    "background",
    "rgba(255, 255, 255, 0.02)"
  );

  streamBtnQuery.style.setProperty(
    "background",
    "rgba(255, 255, 255, 0.02)"
  );

  optionsBtnQuery.style.setProperty(
    "background",
    "rgba(255, 255, 255, 0.02)"
  )
});

streamBtn?.addEventListener("click", () => {
  mainSec.style.setProperty("display", "none");
  searchSec.style.setProperty("display", "none");
  collectionsSec.style.setProperty("display", "none");
  optionsSec.style.setProperty("display", "none");

  streamSec.style.setProperty("display", "block");


  homeBtnQuery.style.setProperty(
    "background",
    "rgba(255, 255, 255, 0.02)"
  );

  streamBtnQuery.style.setProperty(
    "background",
    "rgba(0, 75, 75, 0.75)"
  );

  searchBtnQuery.style.setProperty(
    "background",
    "rgba(255, 255, 255, 0.02)"
  );

  collectionsBtnQuery.style.setProperty(
    "background",
    "rgba(255, 255, 255, 0.02)"
  );

  optionsBtnQuery.style.setProperty(
    "background",
    "rgba(255, 255, 255, 0.02)"
  )
});

optionsBtn?.addEventListener("click", () => {
  mainSec.style.setProperty("display", "none");
  searchSec.style.setProperty("display", "none");
  collectionsSec.style.setProperty("display", "none");

  streamSec.style.setProperty("display", "none");
  optionsSec.style.setProperty("display", "block");


  homeBtnQuery.style.setProperty(
    "background",
    "rgba(255, 255, 255, 0.02)"
  );

  streamBtnQuery.style.setProperty(
    "background",
    "rgba(255, 255, 255, 0.02)"
  );

  searchBtnQuery.style.setProperty(
    "background",
    "rgba(255, 255, 255, 0.02)"
  );

  collectionsBtnQuery.style.setProperty(
    "background",
    "rgba(255, 255, 255, 0.02)"
  );

  optionsBtnQuery.style.setProperty(
    "background",
    "rgba(75, 0, 75, 0.65)"
  )
});

// --------------------------------------------------
// RADIO UI
// --------------------------------------------------

function updateRadioControls() {
  const panel =
    document.querySelector("#radio-now-playing");

  const stationName =
    document.querySelector("#radio-station-name");

  const status =
    document.querySelector("#radio-playback-status");

  if (panel) {
    panel.hidden = !currentStation;
  }

  if (stationName) {
    stationName.textContent =
      currentStation?.name || "No station selected";
  }

  if (status) {
    status.textContent = radioActive
      ? "Playing live"
      : radioPaused
        ? "Paused · Reconnect to resume"
        : "Stopped";
  }

  playBtn?.classList.toggle("is-playing", radioActive);
  nowPlayBtn?.classList.toggle("is-playing", radioActive);



  if (progressSlider) {
    progressSlider.disabled =
      radioActive || radioPaused;
  }
  if (nowProgressSlider) {
    nowProgressSlider.disabled =
      radioActive || radioPaused;
  }

  if (playImage && currentStation) {
    playImage.src = radioActive
      ? "assets/media-playback-pause-symbolic.svg"
      : "assets/media-playback-start-symbolic.svg";
  }
  if (nowPlayImage && currentStation) {
    nowPlayImage.src = radioActive
      ? "assets/media-playback-pause-symbolic.svg"
      : "assets/media-playback-start-symbolic.svg";
  }
}

function showRadioInPlayer() {
  if (!currentStation) return;

  const name = document.querySelector("#music-name");
  const nowName = document.querySelector("#now-playing-name");

  const nowPlayingArtist = document.getElementById("now-playing-artist");
  const nowPlayingThumbnail = document.getElementById("now-playing-image");

  if (nowPlayingThumbnail) {
    nowPlayingThumbnail.replaceChildren();
    nowPlayingThumbnail.textContent = "📻";
  }
  if (nowPlayingArtist) {
    nowPlayingArtist.textContent = "";
  }

  if (name) {
    name.textContent = currentStation.name;
  }
  if (nowName) {
    nowName.textContent = currentStation.name;
  }

  if (currentTime) {
    currentTime.textContent = "LIVE";
  }
  if (nowCurrentTime) {
    nowCurrentTime.textContent = "LIVE";
  }

  if (totalTime) {
    totalTime.textContent = "RADIO";
  }
  if (nowTotalTime) {
    nowTotalTime.textContent = "RADIO";
  }
}

// --------------------------------------------------
// START RADIO
// --------------------------------------------------

async function playRadioStation(station) {
  if (radioBusy || playbackTransitioning) return;

  radioBusy = true;

  try {
    await invoke("stop_track");

    trackLoaded = false;
    previousPlaybackStatus = null;

    await invoke("start_radio", {
      url: station.url_resolved
    });

    currentStation = station;
    radioActive = true;
    radioPaused = false;

    showRadioInPlayer();
    updateRadioControls();

    console.log(
      "[musicplayer Radio] Playing:",
      station.name
    );
  } catch (error) {
    radioActive = false;
    radioPaused = false;

    updateRadioControls();

    console.error(
      "[musicplayer Radio] Start failed:",
      error
    );

    throw error;
  } finally {
    radioBusy = false;
  }
}

// --------------------------------------------------
// PAUSE RADIO
// --------------------------------------------------

async function pauseRadio() {
  if (radioBusy || !radioActive) return;

  radioBusy = true;

  try {
    await invoke("stop_radio");

    radioActive = false;
    radioPaused = true;

    updateRadioControls();
  } catch (error) {
    console.error(
      "[musicplayer Radio] Pause failed:",
      error
    );
  } finally {
    radioBusy = false;
  }
}

// --------------------------------------------------
// STOP RADIO
// --------------------------------------------------

async function stopRadio() {
  if (radioBusy) return;

  radioBusy = true;

  try {
    await invoke("stop_radio");

    radioActive = false;
    radioPaused = false;
    currentStation = null;

    updateRadioControls();

    if (playImage) {
      playImage.src =
        "assets/media-playback-start-symbolic.svg";
    }

    if (currentTime) {
      currentTime.textContent = "0:00";
    }

    if (totalTime) {
      totalTime.textContent = "0:00";
    }

    const name = document.querySelector("#music-name");

    if (name) {
      name.textContent = "Audio Player";
    }

    console.log("[musicplayer Radio] Stopped.");
  } catch (error) {
    console.error(
      "[musicplayer Radio] Stop failed:",
      error
    );
  } finally {
    radioBusy = false;
  }
}


initRadio(playRadioStation);

document.addEventListener("click", event => {
  if (event.target.closest("#radio-stop-btn")) {
    event.preventDefault();
    stopRadio();
  }
});

// --------------------------------------------------
// SIDEBAR TOGGLE SWITCH
// --------------------------------------------------

const sidebarToggle = document.getElementById("sidebar-toggle");
const sidebarToggleSelect = document.querySelector("#sidebar-toggle");
const sideLabel = document.querySelectorAll(".label");
const sidePanel = document.querySelector(".side-panel");
const sideBtnSelect = document.querySelectorAll(".btn");
const mainContent = document.querySelector(".main-content");
const bottomPlayer = document.querySelector(".player");
const mediaInfo = document.querySelector(".media-info");

let toggle = 1;

sidebarToggle.addEventListener("click", function () {
  if (toggle == 0) {
    sideLabel.forEach(element => {
      element.style.setProperty('display', 'none');
    });

    sidePanel.style.setProperty('width', '65px', 'important');

    sidebarToggleSelect.style.setProperty('background-color', 'rgba(0, 0, 0, 0.1)', 'important');

    sideBtnSelect.forEach(element => {
      element.style.setProperty('width', '20px');
    });

    toggle = 1;

    mainContent.style.setProperty('left', '110px');

  } else {

    toggle = 0;

    sideLabel.forEach(element => {
      element.style.setProperty('display', 'block');
    });

    sidePanel.style.setProperty('width', '225px', 'important');

    sidebarToggleSelect.style.setProperty('background-color', 'rgba(56, 35, 25, 0.5)', 'important');

    sideBtnSelect.forEach(element => {
      element.style.setProperty('width', '175px');
    });

    sidebarToggleSelect.style.setProperty('width', '20px', 'important');

    mainContent.style.setProperty('left', '270px');

  }
});

// --------------------------------------------------
// OPTIONS
// --------------------------------------------------

const interfaceBtn = document.getElementById("interface-btn");
const aboutBtn = document.getElementById("about-btn");
const keybindsBtn = document.getElementById("keybinds");

const optionsPage = document.querySelectorAll(".options-page");

const interfacePage = document.querySelector("#interface-section");
const aboutPage = document.querySelector("#about-section");
const keybindsPage = document.querySelector("#keybinds-section");

keybindsBtn.addEventListener('click', function () {
  optionsPage.forEach(element => {
    element.style.setProperty('display', 'none');
  });
  keybindsPage.style.setProperty('display', 'block');
});

aboutBtn.addEventListener("click", function () {
  optionsPage.forEach(element => {
    element.style.setProperty('display', 'none');
  });
  aboutPage.style.setProperty('display', 'block');
});

interfaceBtn.addEventListener("click", function () {
  optionsPage.forEach(element => {
    element.style.setProperty('display', 'none');
  });
  interfacePage.style.setProperty('display', 'block');
});



const uploadBtn = document.getElementById("custom-css-button");
const fileInput = document.getElementById("fileInput");

document.addEventListener('DOMContentLoaded', () => {
  const savedCSS = localStorage.getItem('userCustomCSS');
  if (savedCSS) {
    applyCSSToPage(savedCSS);
  }
});

uploadBtn.addEventListener('click', () => {
   fileInput.click();
 });


fileInput.addEventListener('change', (event) => {
  const file = event.target.files[0];
  if (!file) return;

  const reader = new FileReader();
  reader.onload = (e) => {
    const cssContent = e.target.result;

    applyCSSToPage(cssContent);

    localStorage.setItem('userCustomCSS', cssContent);
  };
  reader.readAsText(file);
});


function applyCSSToPage(cssText) {

  const existingStyle = document.getElementById('custom-runtime-theme');
  if (existingStyle) existingStyle.remove();

  const styleTag = document.createElement('style');
  styleTag.id = 'custom-runtime-theme';
  styleTag.textContent = cssText;

  document.head.appendChild(styleTag);
}
const resetBtn = document.getElementById('reset-css');

resetBtn.addEventListener('click', () => {
  localStorage.removeItem('userCustomCSS');

  const customStyle = document.getElementById('custom-runtime-theme');
  if (customStyle) {
    customStyle.remove();
  }
  document.getElementById('fileInput').value = '';

});

const savedTheme = localStorage.getItem('theme');
const systemPrefersLight = window.matchMedia('(prefers-color-scheme: light)').matches;

if (savedTheme === 'light' || (!savedTheme && systemPrefersLight)) {
  document.documentElement.classList.add('light-theme');
} else {
  document.documentElement.classList.remove('light-theme');
}

document.addEventListener('DOMContentLoaded', () => {
  const toggleBtn = document.getElementById('theme-toggle');
  if (document.documentElement.classList.contains('light-theme')) {
      toggleBtn.textContent = 'Light';
  } else {
    toggleBtn.textContent = 'Dark';
  }
  toggleBtn.addEventListener('click', () => {
    const isLight = document.documentElement.classList.toggle('light-theme');

    localStorage.setItem('theme', isLight ? 'light' : 'dark');
    if (isLight) {
      toggleBtn.textContent = 'Light';
    } else {
      toggleBtn.textContent = 'Dark';
    }
  });
});
const reducedStyle = document.createElement("style");
reducedStyle.id = 'kill-animations-style';
reducedStyle.textContent = `
  *, *::before, *::after {
    animation-duration: 0s !important;
    animation-iteration-count: 1 !important;
    transition-duration: 0s !important;
    scroll-behavior: auto !important;
  }
`;
let animationsDisabled = false;
const reducedToggle = document.getElementById("reduce-animation");
reducedToggle.addEventListener('click', () => {
  if (!animationsDisabled) {
    reducedToggle.innerText = 'Enabled';
    document.head.appendChild(reducedStyle);
    document.getAnimations().forEach(anim => anim.pause());
  } else {
    reducedToggle.innerText = 'Disabled';
    const existingStyle = document.getElementById('kill-animations-style');
    if (existingStyle) existingStyle.remove();
    document.getAnimations().forEach(anim => anim.play());
  }
  animationsDisabled = !animationsDisabled;
});


const sizeInput = document.getElementById('font-size-title');
const headingFont = document.querySelectorAll('h1');

const savedSize = localStorage.getItem('userFontSize') || '35';

sizeInput.value = savedSize;
headingFont.forEach(h1 => {
  h1.style.fontSize = `${savedSize}px`;
});

function updateFontSize() {
  const newSize = sizeInput.value;
  headingFont.forEach(h1 => {
    h1.style.fontSize = `${newSize}px`;
  });
  localStorage.setItem('userFontSize', newSize);
}
sizeInput.addEventListener('input', updateFontSize);

const picker = document.getElementById("color-picker");
const root = document.documentElement;
const lightMode = document.querySelector(".light-theme");
const resetColor = document.getElementById("reset-color");

const STORAGE_KEY = 'userCustomColor';

function initTheme() {
    const savedColor = localStorage.getItem(STORAGE_KEY);

    if (savedColor) {
        root.style.setProperty('--media-background', savedColor);
        lightMode.style.setProperty('--media-background', savedColor);
        picker.value = savedColor;
    } else {
      picker.value = getCSSVariableDefault('--media-background');
    }
}

function getCSSVariableDefault(variableName) {
    return getComputedStyle(root).getPropertyValue(variableName).trim();
}

function getCSSVariableDefaultLight(variableName) {
    return getComputedStyle(lightMode).getPropertyValue(variableName).trim();
}

picker.addEventListener('input', (e) => {
  root.style.setProperty('--theme-color', e.target.value);
  lightMode.style.setProperty('--theme-color', e.target.value);
});

picker.addEventListener('change', (e) => {
    localStorage.setItem(STORAGE_KEY, e.target.value);
});

resetColor.addEventListener('click', () => {
  localStorage.removeItem(STORAGE_KEY);
  root.style.removeProperty('--theme-color');
  lightMode.style.removeProperty('--theme-color');
  picker.value = getCSSVariableDefault('--theme-color');
});

initTheme();

document.addEventListener('DOMContentLoaded', () => {
  const fontChanger = document.getElementById('font-types');
  const storageKey = 'user-selected-font';
  const resetBtn = document.getElementById('reset-font');
  const defaultFont = 'system-ui';
  const savedFont = localStorage.getItem(storageKey);
  if (savedFont) {
    document.documentElement.style.setProperty('--font-family', savedFont);
    fontChanger.value = savedFont;
  }
  fontChanger.addEventListener('change', (event) => {
    const selectedFont = event.target.value;
    document.documentElement.style.setProperty('--font-family', selectedFont);
    localStorage.setItem(storageKey, selectedFont);
    });
  resetBtn.addEventListener('click', () => {
    localStorage.removeItem(storageKey);
    document.documentElement.style.setProperty('--body-font', defaultFont);
    fontChanger.value = defaultFont;
  });
});

// --------------------------------------------------
// NOTIFICATION PLAYING
// --------------------------------------------------

const { listen } = window.__TAURI__.event;

await listen("musicplayer://mpris-play-pause", async () => {
  await togglePlayback();
});

await listen("musicplayer://mpris-play", async () => {
  const state = await invoke("get_player_state");

  if (state.status !== "playing") {
    await togglePlayback();
  }
});

await listen("musicplayer://mpris-pause", async () => {
  const state = await invoke("get_player_state");

  if (state.status === "playing") {
    await togglePlayback();
  }
});

await listen("musicplayer://mpris-next", async () => {

  document.querySelector("#next-btn")?.click();
});

await listen("musicplayer://mpris-previous", async () => {

  document.querySelector("#previous-btn")?.click();
});

// --------------------------------------------------
// MUSIC DRAG & DROP
// --------------------------------------------------

const SUPPORTED_AUDIO_EXTENSIONS = new Set([
  "mp3",
  "flac",
  "wav",
  "ogg",
  "opus",
  "m4a",
  "aac",
  "aiff",
  "aif",
  "wma"
]);

let musicDropInitialized = false;
let musicDropBusy = false;

function isSupportedAudioFile(path) {
  if (typeof path !== "string") return false;

  const extension = path
    .split(/[\\/]/)
    .pop()
    ?.split(".")
    .pop()
    ?.toLowerCase();

  return SUPPORTED_AUDIO_EXTENSIONS.has(extension);
}

function showMusicDropOverlay() {
  document
    .getElementById("music-drop-overlay")
    ?.classList.add("active");
}

function hideMusicDropOverlay() {
  document
    .getElementById("music-drop-overlay")
    ?.classList.remove("active");
}

async function handleDroppedMusic(paths) {
  if (musicDropBusy) return;

  musicDropBusy = true;

  try {
    console.log("[DROP] Received paths:", paths);

    const audioPaths = paths.filter(isSupportedAudioFile);

    if (audioPaths.length === 0) {
      console.warn("[DROP] No supported audio files found.");
      return;
    }

    const existingPaths = new Set(playlist);

    const newPaths = audioPaths.filter(path => {
      if (existingPaths.has(path)) return false;

      existingPaths.add(path);
      return true;
    });

    if (newPaths.length === 0) {
      console.log("[DROP] All files are already in the playlist.");
      return;
    }

    const shouldAutoPlay = !trackLoaded && !currentStation;

    const firstNewIndex = playlist.length;

    playlist.push(...newPaths);

    console.log(
      `[DROP] Added ${newPaths.length} song(s).`,
      newPaths
    );

    if (shouldAutoPlay) {
      console.log("[DROP] Starting first dropped song.");

      const started = await loadSong(firstNewIndex);

      if (!started) {
        console.error("[DROP] loadSong() returned false.");
      }
    } else {
      console.log(
        "[DROP] Songs appended; existing playback preserved."
      );
    }

  } catch (error) {
    console.error("[DROP] Failed:", error);
  } finally {
    musicDropBusy = false;
  }
}

async function initializeMusicDragDrop() {
  if (musicDropInitialized) return;

  const getCurrentWebview =
    window.__TAURI__?.webview?.getCurrentWebview;

  if (!getCurrentWebview) {
    console.error(
      "[musicplayer] Tauri webview API unavailable."
    );
    return;
  }

  const webview = getCurrentWebview();

  await webview.onDragDropEvent(async event => {
    const payload = event.payload;

    switch (payload.type) {
      case "enter":
        showMusicDropOverlay();
        break;

      case "over":
        break;

      case "leave":
        hideMusicDropOverlay();
        break;

      case "drop":
        console.log("[DROP] FILES:", payload.paths);

        hideMusicDropOverlay();

        await handleDroppedMusic(payload.paths);

        break;
    }
  });

  musicDropInitialized = true;

  console.log(
    "[musicplayer] Native music drag & drop initialized."
  );
}

await initializeMusicDragDrop();



// --------------------------------------------------
// NOW PLAYING
// --------------------------------------------------


const musicThumbnailBtn = document.getElementById("music-thumbnail");
const nowPlayingBack = document.getElementById("now-playing-back");
const nowPlaying = document.querySelector(".now-playing");

let v = 0;

musicThumbnailBtn.addEventListener('click', () => {
  nowPlaying.style.setProperty('display', 'flex');
});

nowPlayingBack.addEventListener('click', () => {
  nowPlaying.style.setProperty('display', 'none');
});

const nowPlayingFullscreen = document.getElementById("now-playing-fullscreen");

async function toggleFullscreen() {
  const appWindow = window.__TAURI__.window.getCurrentWindow();
  const isFullscreen = await appWindow.isFullscreen();
  await appWindow.setFullscreen(!isFullscreen);
}

nowPlayingFullscreen.addEventListener('click', toggleFullscreen);

async function exitFullscreen() {
  try {
    const appWindow = window.__TAURI__.window.getCurrentWindow();

    const isFullscreen = await appWindow.isFullscreen();

    if (isFullscreen) {
      await appWindow.setFullscreen(false);
    }
  } catch (error) {
    console.error("Failed to exit fullscreen:", error);
  }
}

window.addEventListener('keydown', async (event) => {
  if (event.key === 'Escape') {
    event.preventDefault();

    await exitFullscreen();
    nowPlaying.style.setProperty('display', 'none');
  }
  if (event.key.toLowerCase() === 'f') {
    event.preventDefault();

    await toggleFullscreen();
    nowPlaying.style.setProperty('display', 'flex');
  }
});

function nowPlayingArtist(newText) {
  const nowArtist = document.getElementById("now-playing-artist");
  if (nowArtist) {
    nowArtist.textContent = newText;
  } else {
    console.error("Element not found.");
  }
}


// ==========================================
// OPEN WITH
// ==========================================

let openWithInitialized = false;
let openWithProcessing = false;

async function processOpenWithFiles() {
  if (openWithProcessing) return;

  openWithProcessing = true;

  try {
    while (true) {
      const paths = await invoke("take_open_with_files");

      if (!paths.length) break;

      console.log("[OPEN WITH] Received:", paths);

      await handleDroppedMusic(paths);
    }
  } catch (error) {
    console.error("[OPEN WITH] Failed:", error);
  } finally {
    openWithProcessing = false;
  }
}

async function initializeOpenWith() {
  if (openWithInitialized) return;

  const { listen } = window.__TAURI__.event;

  await listen("musicplayer://open-with", () => {
    void processOpenWithFiles();
  });

  openWithInitialized = true;

  console.log("[OPEN WITH] Initialized");

  await processOpenWithFiles();
}

await initializeMusicDragDrop();
await initializeOpenWith();
