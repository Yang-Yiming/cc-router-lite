use serde_json::{json, Value as JsonValue};
use std::fs;
use std::path::{Path, PathBuf};

use crate::error::CcrlError;

pub const OAUTH_PROFILE_NAME: &str = "OAuth";

pub fn codex_config_path() -> PathBuf {
    let mut p = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    p.push(".codex");
    p.push("config.toml");
    p
}

pub fn codex_auth_path() -> PathBuf {
    let mut p = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    p.push(".codex");
    p.push("auth.json");
    p
}

pub fn oauth_snapshot_path() -> PathBuf {
    let mut p = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    p.push("ccr-lite");
    p.push("codex-oauth-auth.json");
    p
}

pub fn has_oauth_snapshot(path: &Path) -> bool {
    path.exists()
}

pub fn current_model_provider(path: &Path) -> Result<Option<String>, CcrlError> {
    let root = load_toml(path)?;
    Ok(root
        .as_table()
        .and_then(|table| table.get("model_provider"))
        .and_then(|value| value.as_str())
        .map(ToString::to_string))
}

pub fn set_model_provider(
    path: &Path,
    provider_name: &str,
    base_url: &str,
    wire_api: &str,
    requires_openai_auth: bool,
) -> Result<(), CcrlError> {
    let mut root = load_toml(path)?;
    let table = root
        .as_table_mut()
        .ok_or_else(|| CcrlError::ConfigNotFound(path.display().to_string()))?;

    table.insert(
        "model_provider".to_string(),
        toml::Value::String(provider_name.to_string()),
    );

    let providers = table
        .entry("model_providers")
        .or_insert_with(|| toml::Value::Table(toml::map::Map::new()))
        .as_table_mut()
        .ok_or_else(|| CcrlError::ConfigNotFound(path.display().to_string()))?;

    let provider = providers
        .entry(provider_name.to_string())
        .or_insert_with(|| toml::Value::Table(toml::map::Map::new()))
        .as_table_mut()
        .ok_or_else(|| CcrlError::ConfigNotFound(path.display().to_string()))?;

    provider.insert(
        "name".to_string(),
        toml::Value::String(provider_name.to_string()),
    );
    provider.insert(
        "base_url".to_string(),
        toml::Value::String(base_url.to_string()),
    );
    provider.insert(
        "wire_api".to_string(),
        toml::Value::String(wire_api.to_string()),
    );
    provider.insert(
        "requires_openai_auth".to_string(),
        toml::Value::Boolean(requires_openai_auth),
    );

    write_toml(path, &root)
}

pub fn clear_model_provider(path: &Path) -> Result<(), CcrlError> {
    if !path.exists() {
        return Ok(());
    }

    let mut root = load_toml(path)?;
    if let Some(table) = root.as_table_mut() {
        table.remove("model_provider");
    }
    write_toml(path, &root)
}

pub fn write_api_key_auth(path: &Path, api_key: &str) -> Result<(), CcrlError> {
    let content = serde_json::to_string_pretty(&json!({
        "OPENAI_API_KEY": api_key,
    }))?;
    write_string(path, &content)
}

pub fn current_auth_is_oauth(path: &Path) -> Result<bool, CcrlError> {
    if !path.exists() {
        return Ok(false);
    }
    let root = load_auth_json(path)?;
    Ok(is_oauth_auth(&root))
}

pub fn refresh_oauth_snapshot_if_needed(
    auth_path: &Path,
    snapshot_path: &Path,
) -> Result<bool, CcrlError> {
    if !auth_path.exists() {
        return Ok(false);
    }

    let content = fs::read_to_string(auth_path)?;
    let root: JsonValue =
        serde_json::from_str(&content).map_err(|e| CcrlError::AuthJsonParse(e.to_string()))?;
    if !is_oauth_auth(&root) {
        return Ok(false);
    }

    write_string(snapshot_path, &content)?;
    Ok(true)
}

pub fn restore_oauth_snapshot(auth_path: &Path, snapshot_path: &Path) -> Result<(), CcrlError> {
    if !snapshot_path.exists() {
        return Err(CcrlError::OAuthSnapshotMissing);
    }
    let content = fs::read_to_string(snapshot_path)?;
    write_string(auth_path, &content)
}

fn load_auth_json(path: &Path) -> Result<JsonValue, CcrlError> {
    let content = fs::read_to_string(path)?;
    serde_json::from_str(&content).map_err(|e| CcrlError::AuthJsonParse(e.to_string()))
}

fn is_oauth_auth(root: &JsonValue) -> bool {
    root.get("auth_mode").and_then(|v| v.as_str()) == Some("chatgpt")
        && root
            .get("tokens")
            .and_then(|tokens| tokens.as_object())
            .and_then(|tokens| tokens.get("access_token"))
            .and_then(|value| value.as_str())
            .is_some()
}

fn load_toml(path: &Path) -> Result<toml::Value, CcrlError> {
    if !path.exists() {
        return Ok(toml::Value::Table(toml::map::Map::new()));
    }
    let content = fs::read_to_string(path)?;
    Ok(toml::from_str(&content)?)
}

fn write_toml(path: &Path, value: &toml::Value) -> Result<(), CcrlError> {
    let content = toml::to_string_pretty(value).map_err(|e| {
        CcrlError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            e.to_string(),
        ))
    })?;
    write_string(path, &content)
}

fn write_string(path: &Path, content: &str) -> Result<(), CcrlError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, content)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_set_model_provider_preserves_unrelated_keys() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(
            &path,
            r#"
model = "gpt-5.4"

[projects."/tmp/demo"]
trust_level = "trusted"
"#,
        )
        .unwrap();

        set_model_provider(&path, "fox", "https://example.com/v1", "responses", true).unwrap();

        let content = fs::read_to_string(&path).unwrap();
        let root: toml::Value = toml::from_str(&content).unwrap();
        assert_eq!(root["model"].as_str(), Some("gpt-5.4"));
        assert_eq!(root["model_provider"].as_str(), Some("fox"));
        assert_eq!(root["model_providers"]["fox"]["name"].as_str(), Some("fox"));
        assert_eq!(
            root["model_providers"]["fox"]["base_url"].as_str(),
            Some("https://example.com/v1")
        );
        assert_eq!(
            root["projects"]["/tmp/demo"]["trust_level"].as_str(),
            Some("trusted")
        );
    }

    #[test]
    fn test_clear_model_provider_only_removes_top_level_selection() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(
            &path,
            r#"
model_provider = "fox"

[model_providers.fox]
name = "fox"
"#,
        )
        .unwrap();

        clear_model_provider(&path).unwrap();

        let content = fs::read_to_string(&path).unwrap();
        let root: toml::Value = toml::from_str(&content).unwrap();
        assert!(root.get("model_provider").is_none());
        assert_eq!(root["model_providers"]["fox"]["name"].as_str(), Some("fox"));
    }

    #[test]
    fn test_refresh_oauth_snapshot_if_needed() {
        let dir = tempdir().unwrap();
        let auth = dir.path().join("auth.json");
        let snapshot = dir.path().join("snapshot.json");
        let oauth = serde_json::to_string_pretty(&json!({
            "auth_mode": "chatgpt",
            "OPENAI_API_KEY": null,
            "tokens": { "access_token": "abc" }
        }))
        .unwrap();
        fs::write(&auth, &oauth).unwrap();

        assert!(refresh_oauth_snapshot_if_needed(&auth, &snapshot).unwrap());
        assert_eq!(fs::read_to_string(&snapshot).unwrap(), oauth);
    }

    #[test]
    fn test_write_api_key_auth() {
        let dir = tempdir().unwrap();
        let auth = dir.path().join("auth.json");
        write_api_key_auth(&auth, "sk-test").unwrap();

        let content = fs::read_to_string(&auth).unwrap();
        let root: JsonValue = serde_json::from_str(&content).unwrap();
        assert_eq!(root["OPENAI_API_KEY"], "sk-test");
        assert_eq!(root.as_object().unwrap().len(), 1);
    }

    #[test]
    fn test_restore_oauth_snapshot_missing() {
        let dir = tempdir().unwrap();
        let auth = dir.path().join("auth.json");
        let snapshot = dir.path().join("missing.json");
        assert!(matches!(
            restore_oauth_snapshot(&auth, &snapshot),
            Err(CcrlError::OAuthSnapshotMissing)
        ));
    }
}
