### fast-tts-cli (Rust)

Generate WAV files using Google Cloud Text-to-Speech.

#### Install
- Ensure Rust is installed
- `cargo build --release`
- Binary at `target/release/fast-tts-cli`

Auth options (one of):
- Set `GOOGLE_APPLICATION_CREDENTIALS` to a service account JSON key
- Or run `gcloud auth application-default login`

#### Usage

- Basic:
```bash
cargo run -- "Hello world" hello.wav
```

- Installed binary:
```bash
target/release/fast-tts-cli "Hello world" hello.wav
```

- Options:
```bash
fast-tts-cli --language en-US --gender female --voice en-US-Neural2-F --rate 1.1 --pitch 0.0 --sample-rate 24000 "Hi" hi.wav
```

- List voices:
```bash
fast-tts-cli --list-voices
fast-tts-cli --list-voices --json
```
