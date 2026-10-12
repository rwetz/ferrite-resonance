//! Replace preferences only after the complete new file has been written.
use std::path::Path;

pub fn write(path: &Path, contents: &str) -> Result<(), String> {
    let parent = path.parent().ok_or("Invalid settings path")?;
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let temp = path.with_extension(format!("{}.tmp", std::process::id()));
    let result = std::fs::write(&temp, contents).and_then(|_| std::fs::rename(&temp, path));
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result.map_err(|e| format!("Couldn't save settings: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn directory(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "{}-prefs-{}-{name}",
            env!("CARGO_PKG_NAME"),
            std::process::id()
        ))
    }

    #[test]
    fn replaces_existing_preferences_without_leaving_a_temp_file() {
        let directory = directory("replace");
        let path = directory.join("nested/theme.json");
        write(&path, r#"{"scheme":"mono","appearance":"dark"}"#).unwrap();
        let updated = r#"{"scheme":"harbor","appearance":"system","fps":60,"density":"roomy"}"#;
        write(&path, updated).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), updated);
        assert!(
            !path
                .with_extension(format!("{}.tmp", std::process::id()))
                .exists()
        );
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn failed_replacement_reports_error_and_preserves_destination() {
        let directory = directory("failure");
        let path = directory.join("theme.json");
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(path.join("keep"), "original").unwrap();
        assert!(
            write(&path, "new preferences")
                .unwrap_err()
                .contains("Couldn't save settings")
        );
        assert_eq!(
            std::fs::read_to_string(path.join("keep")).unwrap(),
            "original"
        );
        assert!(
            !path
                .with_extension(format!("{}.tmp", std::process::id()))
                .exists()
        );
        std::fs::remove_dir_all(directory).unwrap();
    }
}
