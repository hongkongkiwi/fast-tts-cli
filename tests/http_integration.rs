use assert_cmd::prelude::*;
use base64::Engine as _;
use httpmock::prelude::*;
use predicates::prelude::*;
use std::fs;
use std::process::Command;
use tempfile::tempdir;

fn read_file(path: &std::path::Path) -> Vec<u8> {
    fs::read(path).unwrap()
}

#[test]
fn synthesize_plain_text_linear16() {
    let server = MockServer::start();

    let synth_mock = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/text:synthesize")
            .header("authorization", "Bearer test-token")
            .json_body_obj(&serde_json::json!({
                "input": {"text": "hello"},
                "voice": {"languageCode": "en-US", "name": "en-US-Neural2-F", "ssmlGender": "FEMALE"},
                "audioConfig": {
                    "audioEncoding": "LINEAR16",
                    "speakingRate": 1.0,
                    "pitch": 0.0,
                    "volumeGainDb": 0.0,
                    "enableLegacyWavHeader": false,
                    "effectsProfileId": ["wearable-class-device"],
                    "sampleRateHertz": 24000
                }
            }));
        then.status(200)
            .json_body_obj(&serde_json::json!({
                "audio_content": base64::engine::general_purpose::STANDARD.encode("WAVDATA")
            }));
    });

    let dir = tempdir().unwrap();
    let out = dir.path().join("hello.wav");

    let mut cmd = Command::cargo_bin("fast-tts-cli").unwrap();
    cmd.env("FAST_TTS_TOKEN", "test-token")
        .env("FAST_TTS_BASE_URL", server.base_url())
        .env_remove("HTTP_PROXY")
        .env_remove("HTTPS_PROXY")
        .env_remove("http_proxy")
        .env_remove("https_proxy")
        .args([
            "--provider",
            "google",
            "--language",
            "en-US",
            "--voice",
            "en-US-Neural2-F",
            "--gender",
            "female",
            "--effects-profile",
            "wearable-class-device",
            "--sample-rate",
            "24000",
            "--encoding",
            "LINEAR16",
            "hello",
            out.to_str().unwrap(),
        ]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Wrote"));

    let bytes = read_file(&out);
    assert_eq!(bytes, b"WAVDATA");
    synth_mock.assert();
}

#[test]
fn synthesize_ssml_mp3() {
    let server = MockServer::start();

    let synth_mock = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/text:synthesize")
            .header("authorization", "Bearer test-token")
            .json_body_obj(&serde_json::json!({
                "input": {"ssml": "<speak>hi</speak>"},
                "voice": {"languageCode": "en-US"},
                "audioConfig": {
                    "audioEncoding": "MP3",
                    "speakingRate": 1.0,
                    "pitch": 0.0,
                    "volumeGainDb": 0.0,
                    "enableLegacyWavHeader": false
                }
            }));
        then.status(200).json_body_obj(&serde_json::json!({
            "audio_content": base64::engine::general_purpose::STANDARD.encode("MP3DATA")
        }));
    });

    let dir = tempdir().unwrap();
    let out = dir.path().join("ssml.mp3");

    let mut cmd = Command::cargo_bin("fast-tts-cli").unwrap();
    cmd.env("FAST_TTS_TOKEN", "test-token")
        .env("FAST_TTS_BASE_URL", server.base_url())
        .env_remove("HTTP_PROXY")
        .env_remove("HTTPS_PROXY")
        .env_remove("http_proxy")
        .env_remove("https_proxy")
        .args([
            "--provider",
            "google",
            "--language",
            "en-US",
            "--encoding",
            "MP3",
            "--ssml",
            "<speak>hi</speak>",
            out.to_str().unwrap(),
        ]);
    cmd.assert().success();

    let bytes = read_file(&out);
    assert_eq!(bytes, b"MP3DATA");
    synth_mock.assert();
}

#[test]
fn list_voices_json() {
    let server = MockServer::start();

    let voices_mock = server.mock(|when, then| {
        when.method(GET)
            .path("/v1/voices")
            .header("authorization", "Bearer fake");
        then.status(200).json_body_obj(&serde_json::json!({
            "voices": [
              {"name": "en-US-Test", "languageCodes": ["en-US"], "ssmlGender": "FEMALE", "naturalSampleRateHertz": 24000}
            ]
        }));
    });

    let mut cmd = Command::cargo_bin("fast-tts-cli").unwrap();
    cmd.env("FAST_TTS_TOKEN", "fake")
        .env("FAST_TTS_BASE_URL", server.base_url())
        .env_remove("HTTP_PROXY")
        .env_remove("HTTPS_PROXY")
        .env_remove("http_proxy")
        .env_remove("https_proxy")
        .args(["--provider", "google", "--list-voices", "--json"]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("\"voices\""));
    voices_mock.assert();
}
