use std::env;
use std::fs;
use std::path::{Path, PathBuf};

pub fn home_dir() -> Option<PathBuf> {
    env::var_os("HOME").map(PathBuf::from)
}

pub fn config_file_path() -> Option<PathBuf> {
    Some(home_dir()?.join(".config/dotsync/config.toml"))
}

pub fn read_destination() -> Option<PathBuf> {
    let content = fs::read_to_string(config_file_path()?).ok()?;
    parse_destination(&content)
}

fn parse_destination(content: &str) -> Option<PathBuf> {
    for line in content.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix("destination") {
            if let Some(value) = rest.trim().strip_prefix('=') {
                let value = value.trim().trim_matches('"');
                if !value.is_empty() {
                    return Some(PathBuf::from(value));
                }
            }
        }
    }
    None
}

pub fn write_destination(destination: &Path) -> Result<(), String> {
    let path = config_file_path().ok_or("HOME environment variable is not set")?;

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Could not create config directory: {e}"))?;
    }

    let content = format!("destination = \"{}\"\n", destination.display());
    fs::write(&path, content)
        .map_err(|e| format!("Could not write config file '{}': {e}", path.display()))
}
