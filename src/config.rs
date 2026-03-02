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
}

pub struct Profile {
    #[allow(dead_code)]
    pub name: String,
    pub url: String,
    pub auth: String,
    pub env: HashMap<String, JsonValue>,
}

pub fn load_config(path: &Path) -> Result<HashMap<String, RawProfile>, CcrlError> {
    if !path.exists() {
        return Err(CcrlError::ConfigNotFound(path.display().to_string()));
    }
    let content = fs::read_to_string(path)?;
    let profiles: HashMap<String, RawProfile> = toml::from_str(&content)?;
    Ok(profiles)
}

fn resolve_value(val: &str) -> Result<String, CcrlError> {
    if let Some(var_name) = val.strip_prefix('$') {
        std::env::var(var_name).map_err(|_| CcrlError::EnvVarNotSet(var_name.to_string()))
    } else {
        Ok(val.to_string())
    }
}

pub fn resolve_profile(name: &str, raw: &RawProfile) -> Result<Profile, CcrlError> {
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
    })
}
