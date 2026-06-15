//! On-disk macro library. Each macro is a JSON file under a `macros/` folder
//! next to the executable (falling back to the current directory).

use std::fs;
use std::path::{Path, PathBuf};

use crate::model::Macro;

/// Directory where macros are stored.
pub fn library_dir() -> PathBuf {
    let base = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("macros")
}

fn ensure_dir() -> std::io::Result<PathBuf> {
    let dir = library_dir();
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Sanitize a macro name into a safe file stem.
fn safe_stem(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' || c == ' ' { c } else { '_' })
        .collect();
    let trimmed = cleaned.trim();
    if trimmed.is_empty() {
        "macro".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Save a macro to `macros/<name>.json`. Returns the path written.
pub fn save(mac: &Macro) -> std::io::Result<PathBuf> {
    let dir = ensure_dir()?;
    let path = dir.join(format!("{}.json", safe_stem(&mac.name)));
    let json = serde_json::to_string_pretty(mac)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    fs::write(&path, json)?;
    Ok(path)
}

/// Load every `*.json` macro in the library directory.
pub fn load_all() -> Vec<Macro> {
    let dir = library_dir();
    let mut macros = Vec::new();
    let Ok(entries) = fs::read_dir(&dir) else {
        return macros;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("json") {
            if let Ok(text) = fs::read_to_string(&path) {
                if let Ok(mac) = serde_json::from_str::<Macro>(&text) {
                    macros.push(mac);
                }
            }
        }
    }
    macros.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    macros
}

/// Delete a macro file by name. Ignores missing files.
pub fn delete(name: &str) -> std::io::Result<()> {
    let path = library_dir().join(format!("{}.json", safe_stem(name)));
    match fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}
