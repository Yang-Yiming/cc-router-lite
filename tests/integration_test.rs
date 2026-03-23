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
