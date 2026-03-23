use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

use crate::config::Target;
use crate::error::CcrlError;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CurrentState {
    pub target: Target,
    pub profile: String,
}

pub fn state_path() -> PathBuf {
    let mut p = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    p.push("ccr-lite");
    p.push(".current");
    p
}

pub fn write_current(state: &CurrentState) -> Result<(), CcrlError> {
    let path = state_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let content = toml::to_string(state).map_err(|e| {
        CcrlError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            e.to_string(),
        ))
    })?;
    fs::write(&path, content)?;
    Ok(())
}

pub fn read_current() -> Option<CurrentState> {
    let path = state_path();
    let content = fs::read_to_string(path).ok()?;
    toml::from_str(&content).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_write_read_current() {
        let dir = tempdir().unwrap();
        std::env::set_var("HOME", dir.path());

        let state = CurrentState {
            target: Target::Claude,
            profile: "test-profile".to_string(),
        };
        write_current(&state).unwrap();
        assert_eq!(read_current(), Some(state));
    }

    #[test]
    fn test_read_current_missing() {
        let dir = tempdir().unwrap();
        std::env::set_var("HOME", dir.path());
        assert_eq!(read_current(), None);
    }
}
