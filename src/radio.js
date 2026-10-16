
const invoke = window.__TAURI__.core.invoke;

let initialized = false;
let searchTimer = null;
let requestVersion = 0;
let onStationSelected = null;

const $ = selector => document.querySelector(selector);

function setRadioStatus(message) {
  const element = $("#radio-status");
  if (element) element.textContent = message;
}

function renderStations(stations) {
  const container = $("#radio-results");
  if (!container) return;

  container.replaceChildren();

  if (!stations.length) {
    setRadioStatus("No stations found. Try another search.");
    return;
  }

  setRadioStatus(`${stations.length} stations found`);

  for (const station of stations) {
    const card = document.createElement("button");
    card.type = "button";
    card.className = "radio-card";

    const symbol = document.createElement("div");
    symbol.className = "radio-card-symbol";
    symbol.textContent = "♫";

    const information = document.createElement("div");
    information.className = "radio-card-info";

    const name = document.createElement("strong");
    name.textContent = station.name || "Unnamed Station";

    const details = document.createElement("span");
    details.textContent = [
      station.country,
      station.codec,
      station.bitrate > 0 ? `${station.bitrate} kbps` : ""
    ].filter(Boolean).join(" · ");

    information.append(name, details);

    const playIcon = document.createElement("span");
    playIcon.className = "radio-card-play";
    playIcon.textContent = "▶";

    card.append(symbol, information, playIcon);

    card.addEventListener("click", () => {
      if (onStationSelected) {
        Promise.resolve(onStationSelected(station))
          .catch(error => {
            console.error("Radio selection failed:", error);
            setRadioStatus(`Cannot select station: ${error}`);
          });
      }
    });

    container.appendChild(card);
  }
}

async function searchRadio() {
  const query = $("#radio-search")?.value.trim() || "";
  const version = ++requestVersion;

  if (!query) {
    $("#radio-results")?.replaceChildren();
    setRadioStatus("Enter a station name to search.");
    return;
  }

  setRadioStatus("Searching radio stations...");

  try {
    const stations = await invoke("search_radio_stations", {
      query
    });

    if (version !== requestVersion) return;

    renderStations(stations);
  } catch (error) {
    if (version !== requestVersion) return;

    console.error("Radio search failed:", error);
    setRadioStatus(`Radio search failed: ${error}`);
  }
}

export function initRadio(onSelect) {
  onStationSelected = onSelect;

  if (initialized) return;
  initialized = true;

  $("#radio-search-btn")?.addEventListener("click", () => {
    searchRadio();
  });

  $("#radio-search")?.addEventListener("input", () => {
    clearTimeout(searchTimer);
    searchTimer = setTimeout(searchRadio, 350);
  });

  $("#radio-search")?.addEventListener("keydown", event => {
    if (event.key === "Enter") {
      clearTimeout(searchTimer);
      searchRadio();
    }
  });
}
