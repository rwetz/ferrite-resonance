//! Per-app preferences. Explicit launcher environment values take precedence.
use ferrite_design::{Appearance, schemes::Scheme, theme};
use gpui::{App, Global, Window};

#[derive(Default)]
struct SaveStatus(Option<String>);
impl Global for SaveStatus {}
pub fn save_error(cx: &App) -> Option<String> {
    cx.try_global::<SaveStatus>().and_then(|s| s.0.clone())
}
fn path() -> Option<std::path::PathBuf> {
    ferrite_design::chrome::config_dir()
        .map(|d| d.join(concat!(env!("CARGO_PKG_NAME"), "-theme.json")))
}
pub fn init(cx: &mut App) {
    let data = path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok());
    let appearance = match data.as_ref().and_then(|d| d["appearance"].as_str()) {
        Some("light") => Appearance::Light,
        Some("system") => Appearance::System,
        _ => Appearance::Dark,
    };
    ferrite_design::init(appearance, cx);
    if let Some(scheme) = data
        .as_ref()
        .and_then(|d| d["scheme"].as_str())
        .and_then(ferrite_design::schemes::by_key)
    {
        theme::set_scheme(scheme, cx);
    }
    if let Some(fps) = data
        .as_ref()
        .and_then(|d| d["fps"].as_u64())
        .filter(|fps| (12..=240).contains(fps))
    {
        ferrite_design::motion::set_fps(fps as u32);
    }
    if let Some(density) = data
        .as_ref()
        .and_then(|d| d["density"].as_str())
        .and_then(|key| {
            theme::Density::ALL
                .into_iter()
                .find(|d| d.label().eq_ignore_ascii_case(key))
        })
    {
        theme::set_density(density, cx);
    }
    theme::apply_env(cx);
}
fn write(cx: &App) -> Result<(), String> {
    let path = path().ok_or("Could not find the settings directory")?;

    let appearance = match theme::appearance(cx) {
        Appearance::Dark => "dark",
        Appearance::Light => "light",
        Appearance::System => "system",
    };
    let data = serde_json::json!({
        "scheme": theme::scheme(cx).key, "appearance": appearance,
        "fps": ferrite_design::motion::fps(), "density": theme::density(cx).label().to_lowercase()
    });
    crate::prefs_store::write(&path, &data.to_string())
}
fn save(cx: &mut App) {
    let error = write(cx).err();
    if let Some(error) = &error {
        eprintln!("{error}");
    }
    cx.set_global(SaveStatus(error));
    cx.refresh_windows();
}
pub fn fps(fps: u32, cx: &mut App) {
    ferrite_design::motion::set_fps(fps);
    save(cx);
}
pub fn density(density: theme::Density, cx: &mut App) {
    theme::set_density(density, cx);
    save(cx);
}
pub fn scheme(scheme: &'static Scheme, cx: &mut App) {
    theme::set_scheme(scheme, cx);
    save(cx);
}
pub fn appearance(appearance: Appearance, window: &mut Window, cx: &mut App) {
    theme::set_appearance(appearance, window, cx);
    save(cx);
}
