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
fast-tts-cli \
  --language en-US \
  --gender female \
  --voice en-US-Neural2-F \
  --rate 1.1 \
  --pitch 0.0 \
  --sample-rate 24000 \
  --encoding LINEAR16 \
  --volume 0.0 \
  --effects-profile wearable-class-device \
  "Hi" hi.wav
```

- List voices:
```bash
fast-tts-cli --list-voices
fast-tts-cli --list-voices --json
```

#### Bulk config (YAML or JSON)

Example YAML (`tts.yaml`):
```yaml
defaults:
  language: en-US
  voice: en-US-Neural2-F
  rate: 1.0
  pitch: 0.0
  encoding: LINEAR16
  sampleRate: 24000
  volumeGainDb: 0
  outputDir: out
items:
  - text: "Welcome to our demo"
    output: intro.wav
  - text: "<speak>Hello <break time='200ms'/> world</speak>"
    ssml: true
    encoding: MP3
    output: ssml.mp3
```

Run:
```bash
fast-tts-cli --config tts.yaml
```
