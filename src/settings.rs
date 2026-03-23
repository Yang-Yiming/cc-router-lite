use serde_json::Value;
use std::fs;
use std::path::Path;

use crate::config::Profile;
use crate::error::CcrlError;

pub fn inject_profile(
    settings_path: &Path,
    profile: &Profile,
    old_keys: &[String],
) -> Result<(), CcrlError> {
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

    for key in old_keys {
        env.remove(key);
    }

    env.insert(
        "ANTHROPIC_BASE_URL".into(),
        Value::String(profile.url.clone()),
    );
    env.insert(
        "ANTHROPIC_AUTH_TOKEN".into(),
        Value::String(profile.auth.clone()),
    );

    for (k, v) in &profile.env {
        env.insert(k.clone(), v.clone());
    }

    if let Some(parent) = settings_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let output = serde_json::to_string_pretty(&root)?;
    fs::write(settings_path, output)?;
    Ok(())
}

pub fn remove_keys(settings_path: &Path, keys: &[String]) -> Result<(), CcrlError> {
    if !settings_path.exists() {
        return Ok(());
    }

    let content = fs::read_to_string(settings_path)?;
    let mut root: Value = serde_json::from_str(&content)?;

    if let Some(env) = root.get_mut("env").and_then(|v| v.as_object_mut()) {
        for key in keys {
            env.remove(key);
        }
    }

    let output = serde_json::to_string_pretty(&root)?;
    fs::write(settings_path, output)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use tempfile::tempdir;

    #[test]
    fn test_inject_profile_new_file() {
        let dir = tempdir().unwrap();
        let settings = dir.path().join("settings.json");

        let profile = Profile {
            name: "test".into(),
            url: "https://api.test.com".into(),
            auth: "sk-test".into(),
            env: HashMap::new(),
            description: None,
            color: None,
        };

        inject_profile(&settings, &profile, &[]).unwrap();

        let content = fs::read_to_string(&settings).unwrap();
        let json: Value = serde_json::from_str(&content).unwrap();
        assert_eq!(json["env"]["ANTHROPIC_BASE_URL"], "https://api.test.com");
        assert_eq!(json["env"]["ANTHROPIC_AUTH_TOKEN"], "sk-test");
    }

    #[test]
    fn test_inject_profile_cleanup_old_keys() {
        let dir = tempdir().unwrap();
        let settings = dir.path().join("settings.json");

        let initial = serde_json::json!({
            "env": {
                "OLD_KEY": "old_value",
                "ANTHROPIC_BASE_URL": "old_url"
            }
        });
        fs::write(&settings, serde_json::to_string_pretty(&initial).unwrap()).unwrap();

        let profile = Profile {
            name: "test".into(),
            url: "https://new.com".into(),
            auth: "sk-new".into(),
            env: HashMap::new(),
            description: None,
            color: None,
        };

        inject_profile(&settings, &profile, &["OLD_KEY".into()]).unwrap();

        let content = fs::read_to_string(&settings).unwrap();
        let json: Value = serde_json::from_str(&content).unwrap();
        assert!(json["env"].get("OLD_KEY").is_none());
        assert_eq!(json["env"]["ANTHROPIC_BASE_URL"], "https://new.com");
    }

    #[test]
    fn test_remove_keys() {
        let dir = tempdir().unwrap();
        let settings = dir.path().join("settings.json");

        let initial = serde_json::json!({
            "env": {
                "KEY1": "value1",
                "KEY2": "value2",
                "KEY3": "value3"
            }
        });
        fs::write(&settings, serde_json::to_string_pretty(&initial).unwrap()).unwrap();

        remove_keys(&settings, &["KEY1".into(), "KEY3".into()]).unwrap();

        let content = fs::read_to_string(&settings).unwrap();
        let json: Value = serde_json::from_str(&content).unwrap();
        assert!(json["env"].get("KEY1").is_none());
        assert_eq!(json["env"]["KEY2"], "value2");
        assert!(json["env"].get("KEY3").is_none());
    }
}
