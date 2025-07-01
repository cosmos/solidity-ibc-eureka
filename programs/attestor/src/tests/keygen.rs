use std::{
    env,
    fs,
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
};

use tempdir::TempDir;
use assert_cmd::prelude::*;
use predicates::prelude::*;

const BIN_NAME: &str = "attestor"; // change if your binary is named differently

fn setup_temp_home() -> TempDir {
    let tmp_dir = TempDir::new("attestor_test_home").expect("failed to create temp home");
    env::set_var("HOME", tmp_dir.path()); // override home dir
    tmp_dir
}

fn key_path(home: &PathBuf) -> PathBuf {
    home.join(".attestor").join("secp256k1")
}

#[test]
fn generates_key_when_none_exists() {
    let tmp_home = setup_temp_home();
    let mut cmd = Command::cargo_bin(BIN_NAME).unwrap();
    cmd.arg("generate").assert().success();

    let key_file = key_path(&tmp_home.into_path());
    assert!(key_file.exists(), "Key file should exist after generation");
}

#[test]
fn does_not_overwrite_key_unless_confirmed() {
    let tmp_home = setup_temp_home();
    let key_file = key_path(&tmp_home.path().to_path_buf());

    // First time: generate key
    let mut cmd = Command::cargo_bin(BIN_NAME).unwrap();
    cmd.arg("generate").assert().success();
    let original = fs::read(&key_file).expect("Key should exist");

    // Second time: simulate user typing "no"
    let mut cmd = Command::cargo_bin(BIN_NAME).unwrap();
    cmd.arg("generate")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    let mut child = cmd.spawn().expect("failed to run generate command");
    let stdin = child.stdin.as_mut().expect("stdin not available");
    writeln!(stdin, "no").expect("failed to write to stdin");
    let _ = child.wait();

    let after = fs::read(&key_file).expect("Key should still exist");
    assert_eq!(original, after, "Key should not be overwritten if 'yes' not typed");
}

#[test]
fn overwrites_key_when_confirmed() {
    let tmp_home = setup_temp_home();
    let key_file = key_path(&tmp_home.path().to_path_buf());

    // First time: generate key
    let mut cmd = Command::cargo_bin(BIN_NAME).unwrap();
    cmd.arg("generate").assert().success();
    let original = fs::read(&key_file).expect("Key should exist");

    // Second time: simulate user typing "yes"
    let mut cmd = Command::cargo_bin(BIN_NAME).unwrap();
    cmd.arg("generate")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    let mut child = cmd.spawn().expect("failed to run generate command");
    let stdin = child.stdin.as_mut().expect("stdin not available");
    writeln!(stdin, "yes").expect("failed to write to stdin");
    let _ = child.wait();

    let after = fs::read(&key_file).expect("Key should still exist");
    assert_ne!(original, after, "Key should be overwritten if 'yes' typed");
}

#[test]
fn fails_if_home_dir_is_invalid() {
    env::set_var("HOME", "/invalid_LOL");

    let mut cmd = Command::cargo_bin(BIN_NAME).unwrap();
    cmd.arg("generate")
        .assert()
        .failure()
        .stderr(predicate::str::contains("Failed to create config directory"));
}
