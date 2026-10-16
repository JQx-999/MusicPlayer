use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RadioStation {
    pub stationuuid: String,
    pub name: String,
    pub url_resolved: String,
    pub favicon: String,
    pub country: String,
    pub codec: String,
    pub bitrate: i32,
}

pub async fn search_stations(query: String) -> Result<Vec<RadioStation>, String> {
    let query = query.trim();

    if query.is_empty() {
        return Ok(Vec::new());
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(12))
        .user_agent("Resonance/0.1 (Internet Radio)")
        .build()
        .map_err(|e| e.to_string())?;

    let stations = client
        .get("https://de1.api.radio-browser.info/json/stations/search")
        .query(&[
            ("name", query),
            ("limit", "40"),
            ("hidebroken", "true"),
            ("order", "clickcount"),
            ("reverse", "true"),
        ])
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .json::<Vec<RadioStation>>()
        .await
        .map_err(|e| e.to_string())?;

    Ok(stations
        .into_iter()
        .filter(|station| !station.url_resolved.trim().is_empty())
        .collect())
}
