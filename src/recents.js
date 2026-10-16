
const STORAGE_KEY = "resonance_recently_played";
const MAX_RECENTS = 12;

function getTrackTitle(track) {
  return (
    track.title?.trim() ||
    track.path.split(/[\\/]/).pop().replace(/\.[^.]+$/, "") ||
    "Unknown Track"
  );
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
        title: typeof track.title === "string" && track.title.trim()
          ? track.title
          : getTrackTitle(track),
        artist: typeof track.artist === "string" && track.artist.trim()
          ? track.artist
          : "Unknown Artist",
        album: typeof track.album === "string" && track.album.trim()
          ? track.album
          : "Unknown Album",
        artwork: typeof track.artwork === "string"
          ? track.artwork
          : null,
        playedAt: Number.isFinite(track.playedAt)
          ? track.playedAt
          : 0
      }))
      .sort((a, b) => b.playedAt - a.playedAt)
      .slice(0, MAX_RECENTS);

  } catch (error) {
    console.error("[Resonance] Failed to load recents:", error);
    return [];
  }
}

export function addRecent(track) {
  if (!track || typeof track.path !== "string" || !track.path.trim()) {
    return getRecents();
  }

  const previous = getRecents();

  const updatedTrack = {
    path: track.path,
    title: getTrackTitle(track),
    artist: track.artist?.trim() || "Unknown Artist",
    album: track.album?.trim() || "Unknown Album",
    artwork: track.artwork || null,
    playedAt: Date.now()
  };

  const updated = [
    updatedTrack,
    ...previous.filter(item => item.path !== track.path)
  ].slice(0, MAX_RECENTS);

  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(updated));
    return updated;
  } catch (error) {
    console.warn(
      "[Resonance] Recents storage limit reached. Retrying without embedded artwork.",
      error
    );

    // Preserve track history and metadata even if artwork is too large.
    const lightweight = updated.map(item => ({
      ...item,
      artwork: null
    }));

    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(lightweight));
      return lightweight;
    } catch (retryError) {
      console.error(
        "[Resonance] Could not save Recents:",
        retryError
      );

      return previous;
    }
  }
}

export function clearRecents() {
  try {
    localStorage.removeItem(STORAGE_KEY);
  } catch (error) {
    console.error("[Resonance] Failed to clear recents:", error);
  }
}
