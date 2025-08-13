# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

### Development Workflow
- `just check` - Complete development check (format, lint, build, test)
- `cargo build --release` - Build release binary (output: `target/release/fast-tts-cli`)
- `cargo test --all` - Run all tests
- `cargo fmt --all` - Format code
- `cargo clippy --all-targets -- -D warnings` - Lint code

### Release Management
- `just bump VERSION` - Update version in Cargo.toml and commit
- `just release VERSION` - Create git tag and push (triggers CI release)
- `just release-patch/minor/major` - Automated semantic version bumping

### Testing
- `cargo test --all` - Run all tests
- `cargo test test_name` - Run specific test
- Tests use `FAST_TTS_TOKEN` and `FAST_TTS_BASE_URL` environment variables for mocking Google TTS API

## Architecture

This is a Rust CLI application for Google Cloud Text-to-Speech with a **single-file monolithic architecture** - all application logic resides in `src/main.rs`.

### Core Components

1. **CLI Interface**: Uses `clap` derive macros for argument parsing. Supports both single synthesis and bulk configuration modes.

2. **Authentication Flow**:
   - Service Account: JWT-based authentication using `GOOGLE_APPLICATION_CREDENTIALS`
   - Application Default Credentials: OAuth2 refresh token flow
   - Test Token: `FAST_TTS_TOKEN` for bypassing authentication during testing

3. **Synthesis Pipeline**:
   - Single: CLI args → `synthesize_to_wav()` → WAV file output
   - Bulk: YAML/JSON config → `run_bulk_from_config()` → Multiple WAV files

4. **API Integration**: Makes POST requests to Google TTS API, decodes base64 audio responses, writes WAV files with automatic directory creation.

### Key Design Patterns

- **Error Handling**: Uses `anyhow::Result` throughout for consistent error propagation
- **Async Runtime**: Tokio multi-threaded runtime for HTTP requests
- **Configuration**: Hierarchical defaults with per-item overrides in bulk mode
- **Testing**: Mock HTTP servers with `httpmock` to avoid actual API calls

### Environment Variables

- `GOOGLE_APPLICATION_CREDENTIALS` - Path to service account JSON key
- `FAST_TTS_BASE_URL` - Override API base URL (default: https://texttospeech.googleapis.com)
- `FAST_TTS_TOKEN` - Test token for bypassing Google authentication

### File Structure

All application code is in `src/main.rs`. Tests are organized in:
- `tests/cli.rs` - CLI argument validation tests
- `tests/bulk.rs` - Bulk configuration parsing tests
- `tests/http_integration.rs` - End-to-end HTTP integration tests

The project uses `Justfile` for task automation instead of Makefile.