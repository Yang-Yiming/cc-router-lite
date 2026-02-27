use std::fs;
use std::path::PathBuf;

use crate::error::CcrlError;

fn state_path() -> PathBuf {
    let mut p = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    p.push("ccr-lite");
    p.push(".current");
    p
}

pub fn write_current(name: &str) -> Result<(), CcrlError> {
    let path = state_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, name)?;
    Ok(())
}

pub fn read_current() -> Option<String> {
    let path = state_path();
    fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}
