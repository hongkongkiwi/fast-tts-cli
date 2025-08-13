# Contributing

- Use Rust stable. Run `just check` before sending PRs.
- For feature branches, include tests and docs and keep commits atomic.
- We use GitHub Actions. Releases are tag-driven (`v*`).

## Dev
- `just check` runs fmt, clippy, build, tests
- Env for testing w/o Google:
  - `FAST_TTS_TOKEN`: fake token
  - `FAST_TTS_BASE_URL`: http mock server base url
