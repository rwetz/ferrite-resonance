//! Remember listening controls without reopening or autoplaying a file.
fn path() -> Option<std::path::PathBuf> {
    ferrite_design::chrome::config_dir().map(|d| d.join("resonance-playback.json"))
}

pub fn load() -> (usize, f32) {
    let data = path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .unwrap_or_default();
    let mode = data["mode"].as_u64().filter(|m| *m < 4).unwrap_or(0) as usize;
    let volume = data["volume"]
        .as_f64()
        .filter(|v| (0.0..=1.0).contains(v))
        .unwrap_or(0.5) as f32;
    (mode, volume)
}

pub fn save(mode: usize, volume: f32) -> Result<(), String> {
    let path = path().ok_or("Could not find the settings directory")?;
    crate::prefs_store::write(
        &path,
        &serde_json::json!({"mode": mode, "volume": volume}).to_string(),
    )
}
