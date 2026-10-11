//! Per-app preferences. Explicit launcher environment values take precedence.
use ferrite_design::{Appearance, schemes::Scheme, theme};
use gpui::{App, Window};
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
    theme::apply_env(cx);
}
fn save(cx: &App) {
    let Some(path) = path() else {
        return;
    };
    let Some(parent) = path.parent() else {
        return;
    };
    if std::fs::create_dir_all(parent).is_err() {
        return;
    }
    let appearance = match theme::appearance(cx) {
        Appearance::Dark => "dark",
        Appearance::Light => "light",
        Appearance::System => "system",
    };
    let data = serde_json::json!({"scheme":theme::scheme(cx).key,"appearance":appearance});
    let temp = path.with_extension(format!("{}.tmp", std::process::id()));
    if std::fs::write(&temp, data.to_string()).is_ok() {
        let _ = std::fs::rename(temp, path);
    }
}
pub fn scheme(scheme: &'static Scheme, cx: &mut App) {
    theme::set_scheme(scheme, cx);
    save(cx);
}
pub fn appearance(appearance: Appearance, window: &mut Window, cx: &mut App) {
    theme::set_appearance(appearance, window, cx);
    save(cx);
}
