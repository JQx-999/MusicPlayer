#[tauri::command]
pub fn pong(path: String) -> Result<(), String> {
    let _ = path;
    Ok(())
}
