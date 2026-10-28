const invoke = window.__TAURI__.core.invoke;

const FOLDER_KEY = "music_directory";


let onTrackSelected = null;
let searchTimer = null;
let searchVersion = 0;
let scanning = false;

const $ = (selector) => document.querySelector(selector);

function setStatus(message) {
  const status = $("#library-status");
  if (status) status.textContent = message;
}
async function renderResults(tracks) {
  const container = $("#library-results");
  if (!container) return;

  container.replaceChildren();

  if (!tracks.length) {
    const empty = document.createElement("p");
    empty.className = "library-empty";
    empty.textContent = "No matching tracks found.";
    container.appendChild(empty);
    return;
  }

  for (const track of tracks) {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "library-track";

    const title = document.createElement("span");
    title.className = "library-track-title";
    title.textContent = track.title;

    const icon = document.createElement("div");
    icon.className = "library-track-icon";
    icon.innerText = "🎝";

    button.append(title, icon);
    button.title = track.path;


    button.addEventListener("click", () => {
      if (onTrackSelected) {
        Promise.resolve(onTrackSelected(track))
          .catch(console.error);
      }
    });

    container.appendChild(button);
  }
}

async function refreshSearch() {
  if (scanning) return;

  const version = ++searchVersion;
  const query = $("#library-search-input")?.value || "";

  try {
    const tracks = await invoke("search_music", { query });

    if (version !== searchVersion) return;

    renderResults(tracks);
    setStatus(`${tracks.length} results displayed`);
  } catch (error) {
    setStatus(`Search failed: ${error}`);
  }
}

async function scanFolder(folder) {
  scanning = true;
  ++searchVersion;
  setStatus("Scanning music directory...");

  try {
    const count = await invoke("scan_music_directory", { folder });

    localStorage.setItem(FOLDER_KEY, folder);

    setStatus(`${count} tracks indexed`);
    scanning = false;

    await refreshSearch();
  } catch (error) {
    scanning = false;
    setStatus(`Scan failed: ${error}`);
    console.error(error);
  }
}

async function chooseDirectory() {
  const folder = await window.__TAURI__.dialog.open({
    directory: true,
    multiple: false,
    title: "Select your Music directory",
  });

  if (!folder) return;

  await scanFolder(folder);
}

export function initMusicSearch(playTrack) {
  onTrackSelected = playTrack;

  $("#choose-music-directory")
    ?.addEventListener("click", () => {
      chooseDirectory().catch(console.error);
    });

  $("#library-search-input")
    ?.addEventListener("input", () => {
      clearTimeout(searchTimer);

      searchTimer = setTimeout(refreshSearch, 120);
    });

  const savedFolder = localStorage.getItem(FOLDER_KEY);

  if (savedFolder) {
    scanFolder(savedFolder);
  }
}
