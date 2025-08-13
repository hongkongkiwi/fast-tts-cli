use assert_cmd::prelude::*;
use predicates::prelude::*;
use std::fs;
use std::process::Command;
use tempfile::tempdir;

#[test]
fn runs_bulk_config_yaml() {
    let dir = tempdir().unwrap();
    let cfg_path = dir.path().join("tts.yaml");
    let out_dir = dir.path().join("out");
    fs::create_dir_all(&out_dir).unwrap();
    fs::write(&cfg_path, r#"
items:
  - text: hello
    output: out/hello.wav
"#).unwrap();

    let mut cmd = Command::cargo_bin("fast-tts-cli").unwrap();
    cmd.env("FAST_TTS_TOKEN", "dummy")
       .env("FAST_TTS_BASE_URL", "http://127.0.0.1:9") // force network error fast
       .args(["--config", cfg_path.to_str().unwrap()]);
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("Error:"));
}
