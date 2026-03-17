use std::fs;
use std::path::PathBuf;

use crate::error::CcrlError;

pub fn state_path() -> PathBuf {
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_write_read_current() {
        let dir = tempdir().unwrap();
        std::env::set_var("HOME", dir.path());

        write_current("test-profile").unwrap();
        assert_eq!(read_current(), Some("test-profile".to_string()));
    }

    #[test]
    fn test_read_current_missing() {
        let dir = tempdir().unwrap();
        std::env::set_var("HOME", dir.path());
        assert_eq!(read_current(), None);
    }
}
