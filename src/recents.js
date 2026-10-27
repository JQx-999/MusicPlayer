
const STORAGE_KEY = "resonance_recently_played";
const MAX_RECENTS = 12;

const DB_NAME = "resonance_artwork";
const STORE_NAME = "covers";
const DB_VERSION = 1;

const ARTWORK_SIZE = 300;
const ARTWORK_QUALITY = 0.82;

let databasePromise = null;

function getTrackTitle(track) {
  return (
    track.title?.trim() ||
    track.path.split(/[\\/]/).pop().replace(/\.[^.]+$/, "") ||
    "Unknown Track"
  );
}

function openArtworkDB() {
  if (!databasePromise) {
    databasePromise = new Promise((resolve, reject) => {
      const request = indexedDB.open(DB_NAME, DB_VERSION);

      request.onupgradeneeded = () => {
        const db = request.result;

        if (!db.objectStoreNames.contains(STORE_NAME)) {
          db.createObjectStore(STORE_NAME);
        }
      };

      request.onsuccess = () => resolve(request.result);
      request.onerror = () => reject(request.error);
    }).catch(error => {
      databasePromise = null;
      throw error;
    });
  }

  return databasePromise;
}

async function saveArtwork(path, blob) {
  const db = await openArtworkDB();

  return new Promise((resolve, reject) => {
    const transaction = db.transaction(STORE_NAME, "readwrite");

    transaction.objectStore(STORE_NAME).put(blob, path);

    transaction.oncomplete = () => resolve();
    transaction.onerror = () => reject(transaction.error);
    transaction.onabort = () => reject(transaction.error);
  });
}

async function readArtwork(path) {
  const db = await openArtworkDB();

  return new Promise((resolve, reject) => {
    const request = db
      .transaction(STORE_NAME, "readonly")
      .objectStore(STORE_NAME)
      .get(path);

    request.onsuccess = () => resolve(request.result || null);
    request.onerror = () => reject(request.error);
  });
}

async function deleteUnusedArtwork(allowedPaths) {
  const db = await openArtworkDB();

  return new Promise((resolve, reject) => {
    const transaction = db.transaction(STORE_NAME, "readwrite");
    const store = transaction.objectStore(STORE_NAME);
    const request = store.openCursor();

    request.onsuccess = () => {
      const cursor = request.result;

      if (!cursor) return;

      if (!allowedPaths.has(cursor.key)) {
        cursor.delete();
      }

      cursor.continue();
    };

    transaction.oncomplete = () => resolve();
    transaction.onerror = () => reject(transaction.error);
    transaction.onabort = () => reject(transaction.error);
  });
}

async function clearArtworkDB() {
  const db = await openArtworkDB();

  return new Promise((resolve, reject) => {
    const transaction = db.transaction(STORE_NAME, "readwrite");

    transaction.objectStore(STORE_NAME).clear();

    transaction.oncomplete = () => resolve();
    transaction.onerror = () => reject(transaction.error);
    transaction.onabort = () => reject(transaction.error);
  });
}

async function optimizeArtwork(artwork) {
  if (!artwork) return null;

  const response = await fetch(artwork);
  const original = await response.blob();

  if (!original.type.startsWith("image/")) {
    return null;
  }

  let bitmap;

  try {
    bitmap = await createImageBitmap(original);

    const scale = Math.min(
      1,
      ARTWORK_SIZE / Math.max(bitmap.width, bitmap.height)
    );

    const canvas = document.createElement("canvas");

    canvas.width = Math.max(1, Math.round(bitmap.width * scale));
    canvas.height = Math.max(1, Math.round(bitmap.height * scale));

    const context = canvas.getContext("2d");

    if (!context) return original;

    context.drawImage(
      bitmap,
      0,
      0,
      canvas.width,
      canvas.height
    );

    const optimized = await new Promise(resolve => {
      canvas.toBlob(resolve, "image/webp", ARTWORK_QUALITY);
    });

    return optimized || original;
  } catch (error) {
    console.warn("[musicplayer] Artwork optimization failed:", error);
    return original;
  } finally {
    bitmap?.close();
  }
}

function blobToDataURL(blob) {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();

    reader.onload = () => resolve(reader.result);
    reader.onerror = () => reject(reader.error);

    reader.readAsDataURL(blob);
  });
}

export function getRecents() {
  try {
    const saved = localStorage.getItem(STORAGE_KEY);
    const parsed = saved ? JSON.parse(saved) : [];

    if (!Array.isArray(parsed)) return [];

    const seen = new Set();

    return parsed
      .filter(track => {
        if (!track || typeof track.path !== "string") return false;
        if (!track.path.trim() || seen.has(track.path)) return false;

        seen.add(track.path);
        return true;
      })
      .map(track => ({
        path: track.path,

        title:
          typeof track.title === "string" && track.title.trim()
            ? track.title
            : getTrackTitle(track),

        artist:
          typeof track.artist === "string" && track.artist.trim()
            ? track.artist
            : "Unknown Artist",

        album:
          typeof track.album === "string" && track.album.trim()
            ? track.album
            : "Unknown Album",

        artwork:
          typeof track.artwork === "string"
            ? track.artwork
            : null,

        playedAt:
          Number.isFinite(track.playedAt)
            ? track.playedAt
            : 0
      }))
      .sort((a, b) => b.playedAt - a.playedAt)
      .slice(0, MAX_RECENTS);

  } catch (error) {
    console.error("[musicplayer] Failed to load recents:", error);
    return [];
  }
}

export async function migrateRecentArtwork() {
  const recents = getRecents();
  const legacy = recents.filter(track => track.artwork);

  if (!legacy.length) return;

  try {
    for (const track of legacy) {
      const existing = await readArtwork(track.path);

      if (!existing) {
        const blob = await optimizeArtwork(track.artwork);

        if (!blob) {
          throw new Error("Could not migrate artwork");
        }

        await saveArtwork(track.path, blob);
      }
    }

    const lightweight = recents.map(({ artwork, ...metadata }) => metadata);

    localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify(lightweight)
    );

    console.log("[musicplayer] Recent artwork migrated successfully.");
  } catch (error) {
    console.warn("[musicplayer] Artwork migration incomplete:", error);
  }
}

export async function getRecentArtwork(path) {
  if (!path) return null;

  try {
    const blob = await readArtwork(path);

    if (blob) {
      return await blobToDataURL(blob);
    }

    return getRecents().find(track => track.path === path)?.artwork || null;
  } catch (error) {
    console.warn("[musicplayer] Failed to retrieve artwork:", error);

    return getRecents().find(track => track.path === path)?.artwork || null;
  }
}

export async function addRecent(track) {
  if (!track || typeof track.path !== "string" || !track.path.trim()) {
    return getRecents();
  }

  await migrateRecentArtwork();

  const previous = getRecents();

  const updatedTrack = {
    path: track.path,
    title: getTrackTitle(track),
    artist: track.artist?.trim() || "Unknown Artist",
    album: track.album?.trim() || "Unknown Album",
    artwork: null,
    playedAt: Date.now()
  };

  if (track.artwork) {
    try {
      const cachedArtwork = await readArtwork(track.path);
      console.log(track.path);
      console.log(track.artwork);
      console.log(track.blob);

      if (!cachedArtwork) {
        const blob = await optimizeArtwork(track.artwork);

        if (blob) {
          await saveArtwork(track.path, blob);
        }
      }
    } catch (error) {
      console.warn("[musicplayer] Could not cache artwork:", error);
    }
  }

  const updated = [
    updatedTrack,
    ...previous.filter(item => item.path !== track.path)
  ].slice(0, MAX_RECENTS);

  const lightweight = updated.map(({ artwork, ...metadata }) => metadata);

  try {
    localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify(lightweight)
    );
  } catch (error) {
    console.error("[musicplayer] Could not save Recents:", error);
    return previous;
  }

  try {
    await deleteUnusedArtwork(
      new Set(lightweight.map(item => item.path))
    );
  } catch (error) {
    console.warn("[musicplayer] Artwork cleanup failed:", error);
  }

  return updated;
}

export async function clearRecents() {
  try {
    localStorage.removeItem(STORAGE_KEY);
    await clearArtworkDB();
  } catch (error) {
    console.error("[musicplayer] Failed to clear recents:", error);
  }
}
