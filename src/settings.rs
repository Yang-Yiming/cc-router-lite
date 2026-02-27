use serde_json::Value;
use std::fs;
use std::path::Path;

use crate::config::Profile;
use crate::error::CcrlError;

pub fn inject_profile(settings_path: &Path, profile: &Profile) -> Result<(), CcrlError> {
    let mut root: Value = if settings_path.exists() {
        let content = fs::read_to_string(settings_path)?;
        serde_json::from_str(&content)?
    } else {
        Value::Object(serde_json::Map::new())
    };

    let env = root
        .as_object_mut()
        .unwrap()
        .entry("env")
        .or_insert_with(|| Value::Object(serde_json::Map::new()))
        .as_object_mut()
        .unwrap();

    env.insert(
        "ANTHROPIC_BASE_URL".into(),
        Value::String(profile.url.clone()),
    );
    env.insert(
        "ANTHROPIC_AUTH_TOKEN".into(),
        Value::String(profile.auth.clone()),
    );

    for (k, v) in &profile.env {
        env.insert(k.clone(), Value::String(v.clone()));
    }

    if let Some(parent) = settings_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let output = serde_json::to_string_pretty(&root)?;
    fs::write(settings_path, output)?;
    Ok(())
}
