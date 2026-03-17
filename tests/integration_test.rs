use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_list_profiles() {
    let dir = tempdir().unwrap();
    let config = dir.path().join("config.toml");

    fs::write(&config, r#"
[test-profile]
url = "https://api.test.com"
auth = "sk-test"
description = "Test profile"
    "#).unwrap();

    Command::cargo_bin("ccrl").unwrap()
        .env("NO_COLOR", "1")
        .arg("--config").arg(&config)
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("test-profile"))
        .stdout(predicate::str::contains("Test profile"));
}

#[test]
fn test_validate_missing_env() {
    let dir = tempdir().unwrap();
    let config = dir.path().join("config.toml");

    fs::write(&config, r#"
[bad-profile]
url = "$MISSING_VAR"
auth = "sk-test"
    "#).unwrap();

    Command::cargo_bin("ccrl").unwrap()
        .env("NO_COLOR", "1")
        .arg("--config").arg(&config)
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
    let config = dir.path().join("config.toml");

    fs::write(&config, "[test]\nurl=\"https://test.com\"\nauth=\"sk-test\"").unwrap();

    Command::cargo_bin("ccrl").unwrap()
        .env("NO_COLOR", "1")
        .arg("--config").arg(&config)
        .arg("diff")
        .arg("nonexistent")
        .assert()
        .failure()
        .stderr(predicate::str::contains("Profile 'nonexistent' not found"));
}
