### fast-tts-cli

Fast, flexible CLI for Google Cloud Text-to-Speech.

#### Features
- Single-shot synthesis with rich options (language, voice, gender, rate, pitch, sample rate, encoding, volume, effects profile)
- SSML or plaintext
- Bulk generation from YAML/JSON configs with defaults and overrides
- Cross-platform binaries via GitHub Releases
- Multi-provider: Google, Gemini (Google AI), OpenAI, Azure, ElevenLabs, Deepgram (+ optional Polly)

#### Install
- Build: `cargo build --release` (binary at `target/release/fast-tts-cli`)
- Or install: `cargo install --path .`

Auth / API keys:
- Google Cloud TTS:
  - `GOOGLE_APPLICATION_CREDENTIALS` -> service-account JSON, or
  - `gcloud auth application-default login`
- Gemini Speech (Google AI):
  - `GEMINI_API_KEY` (required)
  - Optional: `GEMINI_TTS_MODEL` (default: `gemini-1.5-flash-latest`)
  - Note: supported encodings are MP3, OGG_OPUS, LINEAR16 (WAV)
    - MULAW/ALAW are not supported by Gemini

#### Usage
- Basic:
```bash
fast-tts-cli --provider google "Hello world" hello.wav
```

- Options:
```bash
fast-tts-cli \
  --provider google \
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

- Gemini (Google AI) speech generation:
```bash
export GEMINI_API_KEY=...  # required
fast-tts-cli --provider gemini "Hello from Gemini" hello.mp3

# Optional voice and model override
GEMINI_TTS_MODEL=gemini-1.5-flash-latest \
  fast-tts-cli --provider gemini --voice charlie --encoding OGG_OPUS "A short line" out.ogg
```

- List voices:
```bash
fast-tts-cli --provider google --list-voices
fast-tts-cli --provider google --list-voices --json
```

#### Bulk config (YAML or JSON)
`tts.yaml`:
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
Run: `fast-tts-cli --provider google --config tts.yaml`

Note: bulk mode currently uses the Google Cloud TTS path. If you need bulk for other providers, please open an issue.

#### Dev
- just: `just check` (fmt, clippy, build, test)
- Tests mock Google endpoints via `FAST_TTS_BASE_URL` and `FAST_TTS_TOKEN`

#### License
MIT
