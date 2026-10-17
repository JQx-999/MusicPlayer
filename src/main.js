import {
  getRecents,
  addRecent,
  clearRecents
} from "./recents.js";

import { initRadio } from "./radio.js";
import { initCollections } from "./collections.js";
import { initMusicSearch } from "./search.js";

const invoke = window.__TAURI__.core.invoke;

const playBtn = document.querySelector("#play-btn");
const playImage = document.getElementById("play-img");
const volumeSlider = document.querySelector("#volume-slider");
const progressSlider = document.querySelector("#progress-slider");
const currentTime = document.querySelector("#current-time");
const totalTime = document.querySelector("#total-time");
const muteBtn = document.querySelector("#mute-btn");
const audioImg = document.getElementById("audio-img");

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
    console.log("[Resonance] Audio engine:", response);
  } catch (error) {
    console.error("[Resonance] Audio engine error:", error);
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
        `[Resonance] Auto-playing ${nextIndex + 1}/${playlist.length}`
      );

      await loadSong(nextIndex);
    } else {
      console.log("[Resonance] Playlist finished.");

      trackLoaded = false;
      previousPlaybackStatus = "stopped";

      playBtn?.classList.remove("is-playing");

      if (playImage) {
        playImage.src =
          "assets/media-playback-start-symbolic.svg";
      }
    }
  } catch (error) {
    console.error("[Resonance] Auto-next failed:", error);
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

  if (playImage) {
    playImage.src =
      state.status === "playing"
        ? "assets/media-playback-pause-symbolic.svg"
        : "assets/media-playback-start-symbolic.svg";
  }

  if (currentTime) {
    currentTime.textContent = formatTime(state.position);
  }

  if (totalTime) {
    totalTime.textContent = formatTime(state.duration);
  }

  if (progressSlider && !seeking) {
    progressSlider.max = state.duration || 1;
    progressSlider.value = state.position;
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
    console.error("[Resonance] Player polling failed:", error);
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
    }

    if (metadataResult.status === "fulfilled") {
      metadata = metadataResult.value;
    }

    addRecent({
      path,
      title: metadata.title,
      artist: metadata.artist,
      album: metadata.album,
      artwork
    });

    renderRecents();

    const nameElement =
      document.querySelector("#music-name");

    if (nameElement) {
      nameElement.textContent = metadata.title;
    }

    console.log(
      `[Resonance] Playing ${currentTrackIndex + 1}/${playlist.length}:`,
      metadata.title
    );

    return true;
  } catch (error) {
    console.error("[Resonance] Failed to load song:", error);

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
      console.warn("[Resonance] Select a song first.");
      return;
    }

    const state = await invoke("get_player_state");

    if (state.status === "playing") {
      await invoke("pause_track");
    } else if (state.status === "paused") {
      await invoke("resume_track");
    } else {
      await invoke("play_track");
    }

    await updatePlayerUI();
  } catch (error) {
    console.error("[Resonance] Playback error:", error);
  }
}

playBtn?.addEventListener("click", togglePlayback);

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

      if (audioImg) {
        audioImg.src =
          "assets/xsi-audio-volume-high-symbolic.svg";
      }
    }
  } catch (error) {
    console.error("[Resonance] Volume error:", error);
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
    console.error("[Resonance] Mute failed:", error);
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
    console.error("[Resonance] File picker error:", error);
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
  .querySelector("#prev-btn")
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
    console.error("[Resonance] Seeking failed:", error);
  } finally {
    seeking = false;
  }
});

// --------------------------------------------------
// RECENTLY PLAYED
// --------------------------------------------------

function renderRecents() {
  const container =
    document.querySelector("#recents-list");

  if (!container) return;

  const recents = getRecents();

  container.replaceChildren();

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

    if (track.artwork) {
      const image = document.createElement("img");

      image.src = track.artwork;
      image.alt = `${track.title} album artwork`;
      image.loading = "lazy";

      image.addEventListener("error", () => {
        image.remove();
        art.textContent = "♫";
      });

      art.appendChild(image);
    } else {
      art.textContent = "♫";
    }

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
          "[Resonance] Failed to play Recent:",
          error
        );
      }
    });

    container.appendChild(card);
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
  ?.addEventListener("click", () => {
    clearRecents();
    renderRecents();
  });

renderRecents();

// --------------------------------------------------
// PAGE NAVIGATION
// --------------------------------------------------

const homeBtn = document.getElementById("home");
const searchBtn = document.getElementById("search");
const collectionsBtn = document.getElementById("collections");
const streamBtn = document.getElementById("stream");

const sideBtn = document.querySelector(".btn");

const homeBtnQuery = document.querySelector("#home");
const searchBtnQuery = document.querySelector("#search");
const collectionsBtnQuery =
  document.querySelector("#collections");
const streamBtnQuery = document.querySelector("#stream");

const mainSec = document.querySelector(".main-section");
const homeSec = document.querySelector("#home-section");
const searchSec = document.querySelector("#search-section");
const collectionsSec =
  document.querySelector("#collections-section");
const streamSec = document.querySelector("#stream-section");

homeBtn?.addEventListener("click", () => {
  mainSec.style.setProperty("display", "none");
  searchSec.style.setProperty("display", "none");
  collectionsSec.style.setProperty("display", "none");
  streamSec.style.setProperty("display", "none");

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
    "rgb(125, 0, 0)"
  );
});

searchBtn?.addEventListener("click", () => {
  mainSec.style.setProperty("display", "none");
  collectionsSec.style.setProperty("display", "none");
  streamSec.style.setProperty("display", "none");

  searchSec.style.setProperty("display", "block");


  homeBtnQuery.style.setProperty(
    "background",
    "rgba(255, 255, 255, 0.02)"
  );

  searchBtnQuery.style.setProperty(
    "background",
    "rgb(0, 75, 0)"
  );

  collectionsBtnQuery.style.setProperty(
    "background",
    "rgba(255, 255, 255, 0.02)"
  );

  streamBtnQuery.style.setProperty(
    "background",
    "rgba(255, 255, 255, 0.02)"
  );
});

collectionsBtn?.addEventListener("click", () => {
  mainSec.style.setProperty("display", "none");
  searchSec.style.setProperty("display", "none");
  streamSec.style.setProperty("display", "none");

  collectionsSec.style.setProperty("display", "block");


  homeBtnQuery.style.setProperty(
    "background",
    "rgba(255, 255, 255, 0.02)"
  );

  collectionsBtnQuery.style.setProperty(
    "background",
    "rgb(0, 0, 100)"
  );

  searchBtnQuery.style.setProperty(
    "background",
    "rgba(255, 255, 255, 0.02)"
  );

  streamBtnQuery.style.setProperty(
    "background",
    "rgba(255, 255, 255, 0.02)"
  );
});

streamBtn?.addEventListener("click", () => {
  mainSec.style.setProperty("display", "none");
  searchSec.style.setProperty("display", "none");
  collectionsSec.style.setProperty("display", "none");

  streamSec.style.setProperty("display", "block");


  homeBtnQuery.style.setProperty(
    "background",
    "rgba(255, 255, 255, 0.02)"
  );

  streamBtnQuery.style.setProperty(
    "background",
    "rgb(0, 75, 75)"
  );

  searchBtnQuery.style.setProperty(
    "background",
    "rgba(255, 255, 255, 0.02)"
  );

  collectionsBtnQuery.style.setProperty(
    "background",
    "rgba(255, 255, 255, 0.02)"
  );
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

  if (progressSlider) {
    progressSlider.disabled =
      radioActive || radioPaused;
  }

  if (playImage && currentStation) {
    playImage.src = radioActive
      ? "assets/media-playback-pause-symbolic.svg"
      : "assets/media-playback-start-symbolic.svg";
  }
}

function showRadioInPlayer() {
  if (!currentStation) return;

  const name = document.querySelector("#music-name");

  if (name) {
    name.textContent = currentStation.name;
  }

  if (currentTime) {
    currentTime.textContent = "LIVE";
  }

  if (totalTime) {
    totalTime.textContent = "RADIO";
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
      "[Resonance Radio] Playing:",
      station.name
    );
  } catch (error) {
    radioActive = false;
    radioPaused = false;

    updateRadioControls();

    console.error(
      "[Resonance Radio] Start failed:",
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
      "[Resonance Radio] Pause failed:",
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

    console.log("[Resonance Radio] Stopped.");
  } catch (error) {
    console.error(
      "[Resonance Radio] Stop failed:",
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

let toggle = 0;
sidebarToggle.addEventListener("click", function () {
  if (toggle == 0) {
    console.log("nigga");
    sideLabel.forEach(element => {
      element.style.setProperty('display', 'none');
    });
    sidePanel.style.setProperty('width', '70px', 'important');
    sidebarToggleSelect.style.setProperty('background-color', 'rgba(0, 0, 0, 0.1)', 'important');
    sideBtnSelect.forEach(element => {
      element.style.setProperty('width', '20px');
    });
    toggle = 1;
    mainContent.style.setProperty('margin-left', '125px');
    bottomPlayer.style.setProperty('margin-left', '125px');
    mediaInfo.style.setProperty('margin-left', '140px');
  } else {
    toggle = 0;
    sideLabel.forEach(element => {
      element.style.setProperty('display', 'block');
    });
    sidePanel.style.setProperty('width', '225px', 'important');
    sidebarToggleSelect.style.setProperty('background-color', 'rgba(56, 35, 25, 1)', 'important');
    sideBtnSelect.forEach(element => {
      element.style.setProperty('width', '175px');
    });
    sidebarToggleSelect.style.setProperty('width', '20px', 'important');
    mainContent.style.setProperty('margin-left', '285px');
    bottomPlayer.style.setProperty('margin-left', '285px');
    mediaInfo.style.setProperty('margin-left', '300px');
  }
});
