use assert_cmd::prelude::*;
use predicates::prelude::*;
use std::process::Command;
use tempfile::tempdir;

#[test]
fn prints_help_without_args() {
    let mut cmd = Command::cargo_bin("fast-tts-cli").unwrap();
    cmd.arg("--help");
    cmd.assert().success().stdout(predicate::str::contains("fast-tts"));
}

#[test]
fn validates_extension_vs_encoding() {
    let dir = tempdir().unwrap();
    let out = dir.path().join("bad.mp3");
    let mut cmd = Command::cargo_bin("fast-tts-cli").unwrap();
    cmd.args(["hello", out.to_str().unwrap(), "--encoding", "LINEAR16"]);
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("does not match encoding"));
}
