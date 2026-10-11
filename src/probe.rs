//! Optional native-window verification receipt, used by release smoke checks.
pub fn record(detail: String) {
    let Some(path) = std::env::var_os("FERRITE_UI_RECEIPT") else {
        return;
    };
    // Only record the first populated render; normal launches do no file I/O.
    static RECORDED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if RECORDED.swap(true, std::sync::atomic::Ordering::Relaxed) {
        return;
    }
    let _ = std::fs::write(path, detail);
}
