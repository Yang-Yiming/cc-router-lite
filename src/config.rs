use serde::Deserialize;
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::error::CcrlError;

#[derive(Debug, Deserialize)]
pub struct RawProfile {
    pub url: String,
    pub auth: String,
    #[serde(default)]
    pub env: HashMap<String, toml::Value>,
    pub description: Option<String>,
    pub color: Option<String>,
}

pub struct Profile {
    #[allow(dead_code)]
    pub name: String,
    pub url: String,
    pub auth: String,
    pub env: HashMap<String, JsonValue>,
    pub color: Option<String>,
}

pub fn load_config(path: &Path) -> Result<HashMap<String, RawProfile>, CcrlError> {
    if !path.exists() {
        return Err(CcrlError::ConfigNotFound(path.display().to_string()));
    }
    let content = fs::read_to_string(path)?;
    let profiles: HashMap<String, RawProfile> = toml::from_str(&content)?;
    Ok(profiles)
}

fn validate_color(color: &str) -> Result<(), CcrlError> {
    match color {
        "red" | "green" | "yellow" | "blue" | "magenta" | "cyan" | "white" | "black" => Ok(()),
        _ => Err(CcrlError::InvalidColor(color.to_string()))
    }
}

fn resolve_value(val: &str) -> Result<String, CcrlError> {
    if let Some(var_name) = val.strip_prefix('$') {
        std::env::var(var_name).map_err(|_| CcrlError::EnvVarNotSet(var_name.to_string()))
    } else {
        Ok(val.to_string())
    }
}

pub fn resolve_profile(name: &str, raw: &RawProfile) -> Result<Profile, CcrlError> {
    if let Some(c) = &raw.color {
        validate_color(c)?;
    }
    let url = resolve_value(&raw.url)?;
    let auth = resolve_value(&raw.auth)?;
    let mut env = HashMap::new();
    for (k, v) in &raw.env {
        let json_val = match v {
            toml::Value::String(s) => JsonValue::String(resolve_value(s)?),
            toml::Value::Integer(i) => serde_json::json!(i),
            toml::Value::Float(f) => serde_json::json!(f),
            toml::Value::Boolean(b) => JsonValue::Bool(*b),
            other => JsonValue::String(other.to_string()),
        };
        env.insert(k.clone(), json_val);
    }
    Ok(Profile {
        name: name.to_string(),
        url,
        auth,
        env,
        color: raw.color.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_value_literal() {
        assert_eq!(resolve_value("https://api.anthropic.com").unwrap(), "https://api.anthropic.com");
    }

    #[test]
    fn test_resolve_value_env_var() {
        std::env::set_var("TEST_VAR", "test_value");
        assert_eq!(resolve_value("$TEST_VAR").unwrap(), "test_value");
    }

    #[test]
    fn test_resolve_value_missing_env() {
        std::env::remove_var("MISSING_VAR");
        assert!(matches!(
            resolve_value("$MISSING_VAR"),
            Err(CcrlError::EnvVarNotSet(_))
        ));
    }

    #[test]
    fn test_resolve_profile() {
        std::env::set_var("TEST_AUTH", "sk-test");
        let raw = RawProfile {
            url: "https://api.test.com".into(),
            auth: "$TEST_AUTH".into(),
            env: HashMap::new(),
            description: None,
            color: None,
        };
        let profile = resolve_profile("test", &raw).unwrap();
        assert_eq!(profile.url, "https://api.test.com");
        assert_eq!(profile.auth, "sk-test");
    }
}
