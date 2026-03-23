use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::tempdir;

fn write_global_config(dir: &std::path::Path, content: &str) -> std::path::PathBuf {
    let config = dir.join("config.toml");
    fs::write(&config, content).unwrap();
    config
}

fn write_claude_profiles(dir: &std::path::Path, content: &str) {
    fs::write(dir.join("claude.toml"), content).unwrap();
}

fn write_codex_profiles(dir: &std::path::Path, content: &str) {
    fs::write(dir.join("codex.toml"), content).unwrap();
}

fn write_codex_runtime(home: &std::path::Path, config: &str, auth: &str) {
    let codex_dir = home.join(".codex");
    fs::create_dir_all(&codex_dir).unwrap();
    fs::write(codex_dir.join("config.toml"), config).unwrap();
    fs::write(codex_dir.join("auth.json"), auth).unwrap();
}

fn xdg_config_home(home: &std::path::Path) -> std::path::PathBuf {
    home.join(".config")
}

fn find_named_file(root: &std::path::Path, needle: &str) -> Option<std::path::PathBuf> {
    for entry in fs::read_dir(root).ok()? {
        let entry = entry.ok()?;
        let path = entry.path();
        if path.is_file() && path.file_name().and_then(|name| name.to_str()) == Some(needle) {
            return Some(path);
        }
        if path.is_dir() {
            if let Some(found) = find_named_file(&path, needle) {
                return Some(found);
            }
        }
    }
    None
}

#[test]
fn test_list_profiles() {
    let dir = tempdir().unwrap();
    let config = write_global_config(dir.path(), "default_target = \"claude\"\n");
    write_claude_profiles(
        dir.path(),
        r#"
[test-profile]
url = "https://api.test.com"
auth = "sk-test"
description = "Test profile"
        "#,
    );

    Command::cargo_bin("ccrl")
        .unwrap()
        .env("NO_COLOR", "1")
        .arg("--config")
        .arg(&config)
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("test-profile"))
        .stdout(predicate::str::contains("Test profile"));
}

#[test]
fn test_validate_missing_env() {
    let dir = tempdir().unwrap();
    let config = write_global_config(dir.path(), "default_target = \"claude\"\n");
    write_claude_profiles(
        dir.path(),
        r#"
[bad-profile]
url = "$MISSING_VAR"
auth = "sk-test"
        "#,
    );

    Command::cargo_bin("ccrl")
        .unwrap()
        .env("NO_COLOR", "1")
        .arg("--config")
        .arg(&config)
        .arg("validate")
        .assert()
        .success()
        .stdout(predicate::str::contains("✗"))
        .stdout(predicate::str::contains("bad-profile"))
        .stdout(predicate::str::contains("MISSING_VAR"));
}

#[test]
fn test_diff_nonexistent_profile() {
    let dir = tempdir().unwrap();
    let config = write_global_config(dir.path(), "default_target = \"claude\"\n");
    write_claude_profiles(
        dir.path(),
        "[test]\nurl=\"https://test.com\"\nauth=\"sk-test\"\n",
    );

    Command::cargo_bin("ccrl")
        .unwrap()
        .env("NO_COLOR", "1")
        .arg("--config")
        .arg(&config)
        .arg("diff")
        .arg("nonexistent")
        .assert()
        .failure()
        .stderr(predicate::str::contains("Profile 'nonexistent' not found"));
}

#[test]
fn test_invalid_global_config_field() {
    let dir = tempdir().unwrap();
    let config = write_global_config(dir.path(), "target = \"codex\"\n");
    write_claude_profiles(dir.path(), "");

    Command::cargo_bin("ccrl")
        .unwrap()
        .env("NO_COLOR", "1")
        .arg("--config")
        .arg(&config)
        .arg("list")
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown field `target`"));
}

#[test]
fn test_list_codex_profiles_includes_oauth() {
    let dir = tempdir().unwrap();
    let config = write_global_config(dir.path(), "default_target = \"codex\"\n");
    write_codex_profiles(
        dir.path(),
        r#"
[fox]
url = "https://code.newcli.com/codex/v1"
auth = "sk-fox"
description = "Fox"
        "#,
    );
    write_codex_runtime(
        dir.path(),
        "model = \"gpt-5.4\"\n",
        r#"{"auth_mode":"chatgpt","OPENAI_API_KEY":null,"tokens":{"access_token":"abc"}}"#,
    );

    Command::cargo_bin("ccrl")
        .unwrap()
        .env("NO_COLOR", "1")
        .env("HOME", dir.path())
        .env("XDG_CONFIG_HOME", xdg_config_home(dir.path()))
        .arg("--config")
        .arg(&config)
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("fox"))
        .stdout(predicate::str::contains("OAuth"));
}

#[test]
fn test_set_codex_profile_updates_provider_auth_and_snapshot() {
    let dir = tempdir().unwrap();
    let config = write_global_config(dir.path(), "default_target = \"codex\"\n");
    write_codex_profiles(
        dir.path(),
        r#"
[fox]
url = "https://code.newcli.com/codex/v1"
auth = "sk-fox"
wire_api = "responses"
requires_openai_auth = true
        "#,
    );
    let oauth_auth =
        r#"{"auth_mode":"chatgpt","OPENAI_API_KEY":null,"tokens":{"access_token":"abc"}}"#;
    write_codex_runtime(dir.path(), "model = \"gpt-5.4\"\n", oauth_auth);

    Command::cargo_bin("ccrl")
        .unwrap()
        .env("NO_COLOR", "1")
        .env("HOME", dir.path())
        .env("XDG_CONFIG_HOME", xdg_config_home(dir.path()))
        .arg("--config")
        .arg(&config)
        .arg("set")
        .arg("fox")
        .assert()
        .success();

    let codex_config = fs::read_to_string(dir.path().join(".codex/config.toml")).unwrap();
    assert!(codex_config.contains("model_provider = \"fox\""));
    assert!(codex_config.contains("[model_providers.fox]"));
    assert!(codex_config.contains("base_url = \"https://code.newcli.com/codex/v1\""));

    let auth = fs::read_to_string(dir.path().join(".codex/auth.json")).unwrap();
    assert!(auth.contains("\"OPENAI_API_KEY\": \"sk-fox\""));
    assert!(!auth.contains("\"tokens\""));

    let snapshot =
        fs::read_to_string(find_named_file(dir.path(), "codex-oauth-auth.json").unwrap()).unwrap();
    assert_eq!(snapshot, oauth_auth);
}

#[test]
fn test_set_codex_oauth_restores_snapshot_and_clears_provider() {
    let dir = tempdir().unwrap();
    let config = write_global_config(dir.path(), "default_target = \"codex\"\n");
    write_codex_profiles(
        dir.path(),
        r#"
[fox]
url = "https://code.newcli.com/codex/v1"
auth = "sk-fox"
        "#,
    );
    let oauth_auth =
        r#"{"auth_mode":"chatgpt","OPENAI_API_KEY":null,"tokens":{"access_token":"abc"}}"#;
    write_codex_runtime(dir.path(), "model = \"gpt-5.4\"\n", oauth_auth);

    Command::cargo_bin("ccrl")
        .unwrap()
        .env("NO_COLOR", "1")
        .env("HOME", dir.path())
        .env("XDG_CONFIG_HOME", xdg_config_home(dir.path()))
        .arg("--config")
        .arg(&config)
        .arg("set")
        .arg("fox")
        .assert()
        .success();

    Command::cargo_bin("ccrl")
        .unwrap()
        .env("NO_COLOR", "1")
        .env("HOME", dir.path())
        .env("XDG_CONFIG_HOME", xdg_config_home(dir.path()))
        .arg("--config")
        .arg(&config)
        .arg("set")
        .arg("OAuth")
        .assert()
        .success();

    let codex_config = fs::read_to_string(dir.path().join(".codex/config.toml")).unwrap();
    assert!(!codex_config.contains("model_provider ="));

    let auth = fs::read_to_string(dir.path().join(".codex/auth.json")).unwrap();
    assert_eq!(auth, oauth_auth);

    Command::cargo_bin("ccrl")
        .unwrap()
        .env("NO_COLOR", "1")
        .env("HOME", dir.path())
        .env("XDG_CONFIG_HOME", xdg_config_home(dir.path()))
        .arg("now")
        .assert()
        .success()
        .stdout(predicate::str::contains("codex/OAuth"));
}

#[test]
fn test_set_codex_oauth_without_snapshot_fails() {
    let dir = tempdir().unwrap();
    let config = write_global_config(dir.path(), "default_target = \"codex\"\n");
    write_codex_profiles(dir.path(), "");
    write_codex_runtime(
        dir.path(),
        "model = \"gpt-5.4\"\n",
        r#"{"OPENAI_API_KEY":"sk-test"}"#,
    );

    Command::cargo_bin("ccrl")
        .unwrap()
        .env("NO_COLOR", "1")
        .env("HOME", dir.path())
        .env("XDG_CONFIG_HOME", xdg_config_home(dir.path()))
        .arg("--config")
        .arg(&config)
        .arg("set")
        .arg("OAuth")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "No OAuth auth snapshot is available",
        ));
}
