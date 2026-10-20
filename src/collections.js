
const invoke = window.__TAURI__.core.invoke;

let selectedPlaylistId = null;
let playPaths = null;
let initialized = false;

const $ = (selector) => document.querySelector(selector);

function filename(path) {
  return path.split(/[\\/]/).pop();
}

async function getPlaylists() {
  return invoke("list_playlists");
}

async function renderCollections() {
  const grid = $("#playlist-grid");
  if (!grid) return;

  const playlists = await getPlaylists();
  grid.replaceChildren();

  for (const playlist of playlists) {
    const card = document.createElement("button");
    card.type = "button";
    card.className = "playlist-card";

    const art = document.createElement("div");
    art.className = "playlist-card-art";
    art.textContent = "♫";

    const name = document.createElement("strong");
    name.textContent = playlist.name;

    const count = document.createElement("span");
    count.textContent = `${playlist.tracks.length} songs`;

    card.append(art, name, count);

    card.addEventListener("click", () => {
      openPlaylist(playlist.id).catch(console.error);
    });

    grid.appendChild(card);
  }
}

async function openPlaylist(id) {
  const playlists = await getPlaylists();
  const playlist = playlists.find(item => item.id === id);

  if (!playlist) {
    selectedPlaylistId = null;
    await renderCollections();
    return;
  }

  selectedPlaylistId = id;

  $("#playlist-grid").style.setProperty('height', '0');
  $("#playlist-grid").style.setProperty('visibility', 'hidden');
  $("#playlist-detail").hidden = false;
  $("#playlist-detail-name").textContent = playlist.name;

  const container = $("#playlist-tracks");
  container.replaceChildren();

  for (const [index, path] of playlist.tracks.entries()) {
    const row = document.createElement("div");
    row.className = "playlist-track-row";

    const play = document.createElement("button");
    play.type = "button";
    play.textContent = `${index + 1}. ${filename(path)}`;

    play.addEventListener("click", () => {
      playPaths?.(playlist.tracks, index);
    });

    const remove = document.createElement("button");
    remove.type = "button";
    remove.textContent = "Remove";

    remove.addEventListener("click", async () => {
      await invoke("remove_playlist_track", { id, path });
      await openPlaylist(id);
    });

    row.append(play, remove);
    container.appendChild(row);
  }
}

export async function refreshCollections() {
  if (!$("#playlist-grid")) return;

  if (selectedPlaylistId) {
    await openPlaylist(selectedPlaylistId);

  } else {
    await renderCollections();
  }
}

// Call only after the Collections HTML exists.
export function initCollections(onPlayPaths) {
  playPaths = onPlayPaths;

  if (initialized) {
    refreshCollections().catch(console.error);
    return;
  }

  initialized = true;

  $("#new-playlist")?.addEventListener("click", async () => {
    const name = prompt("Playlist name:");
    if (!name?.trim()) return;

    await invoke("create_playlist", { name });
    await renderCollections();
  });

  $("#playlist-back")?.addEventListener("click", async () => {
    selectedPlaylistId = null;
    $("#playlist-detail").hidden = true;
    $("#playlist-grid").style.setProperty('visibility', 'visible');
    $("#playlist-grid").style.setProperty('height', '100%');
    await renderCollections();
  });

  $("#playlist-play-all")?.addEventListener("click", async () => {
    const playlist = (await getPlaylists())
      .find(item => item.id === selectedPlaylistId);

    if (playlist?.tracks.length) {
      await playPaths?.(playlist.tracks, 0);
    }
  });

  $("#playlist-add-track")?.addEventListener("click", async () => {
    const id = selectedPlaylistId;
    if (!id) return;

    const selected = await window.__TAURI__.dialog.open({
      multiple: true,
      directory: false,
      filters: [{
        name: "Music",
        extensions: ["mp3", "flac", "wav", "ogg", "m4a", "aac", "opus"]
      }]
    });

    if (!selected) return;

    const paths = Array.isArray(selected) ? selected : [selected];

    for (const path of paths) {
      await invoke("add_playlist_track", { id, path });
    }

    await openPlaylist(id);
  });

  $("#playlist-delete")?.addEventListener("click", async () => {
    const id = selectedPlaylistId;
    if (!id || !confirm("Delete this playlist?")) return;

    await invoke("delete_playlist", { id });

    selectedPlaylistId = null;
    $("#playlist-detail").hidden = true;
    $("#playlist-grid").hidden = false;

    await renderCollections();
  });

  refreshCollections().catch(console.error);
}
