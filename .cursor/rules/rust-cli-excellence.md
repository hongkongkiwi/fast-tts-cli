# Rule: Build an Excellent, Publishable Rust CLI

This rule guides building a production-grade, user-friendly, and publishable Rust CLI. Apply it whenever creating or evolving a CLI tool in this repo.

## Objectives
- Deliver a fast, reliable, cross-platform CLI with excellent UX.
- Keep core logic testable in a library crate; keep I/O at the edges.
- Provide first-class docs, shell completions, and man pages.
- Ship reproducible, signed release artifacts for macOS, Linux, and Windows.
- Maintain high code quality, diagnostics, and security posture.

## Architectural Principles
- Separate concerns:
  - `src/lib.rs`: core logic (pure, testable, no stdout/stderr).
  - `src/cli.rs`: argument parsing with `clap` (derive API).
  - `src/main.rs`: wiring only (parse, init logging/config, call library, map errors to exit codes).
  - `src/error.rs`: error types if you need domain errors; otherwise use `anyhow` at edges.
- Prefer composition over deep inheritance-like patterns; keep functions small and explicit.
- Avoid global mutable state. Use dependency injection via function parameters or builders.
- Keep platform-specific code behind small shims; normalize paths and newlines.

## User Experience (UX)
- Provide clear `--help` with examples per subcommand.
- Support verbosity flags: `-v/--verbose` (repeatable) and `-q/--quiet`.
- Support color control: `--color=auto|always|never` (respect `NO_COLOR`).
- Support output selection: `--format=plain|json|yaml` where applicable.
- Provide `--output/-o <path>` for file output when useful.
- Use human-friendly defaults; do not surprise users. Confirm destructive actions or require `--yes`.
- Exit codes: `0` success; `1` generic error; reserve other codes for specific classes (document in `README`).
- Stream large I/O; avoid loading everything into memory.

## Error Handling & Diagnostics
- Use `anyhow` for application-level errors, `thiserror` for reusable domain errors.
- Prefer returning `Result<T, E>`; do not `unwrap` in production paths.
- Use `miette` or rich error reports for CLI-facing diagnostics when helpful.
- Log with `tracing` + `tracing-subscriber` using env-filter and JSON support.
- Map errors to user-friendly messages; show `--verbose` stack traces only when requested.

## Configuration Layering
Order of precedence (highest wins):
1) CLI flags
2) Environment variables (with a prefix, e.g., `APP_`)
3) Config file (XDG on Unix, `%APPDATA%` on Windows)
4) Built-in defaults

Recommend `config` or `figment` for layering. Use `directories` to locate config dirs.

## Dependencies (curated)
- CLI: `clap` (derive), `clap_complete`, `clap_mangen`
- Errors: `anyhow`, `thiserror`, optional `miette`
- Logging: `tracing`, `tracing-subscriber`
- Serialization: `serde`, `serde_json`, optionally `serde_yaml`, `toml`
- UX: `indicatif` (progress), `humantime` (durations)
- Files/paths: `camino`, `fs-err`, `tempfile`, `directories`
- Misc: `which`, `once_cell`
- Async (if needed): `tokio` (full features only when justified)

Keep the dependency tree small; justify each new crate.

## Testing Strategy
- Unit tests in library modules.
- Integration tests in `tests/` using `assert_cmd` and `predicates`.
- Snapshot tests for CLI output with `insta` (plain and JSON variants).
- Golden files for complex outputs; store under `tests/data/`.
- Add smoke test for `--help`, `--version`, and basic subcommands.

## Performance & Reliability
- Measure before optimizing. Add benches if critical.
- Use streaming readers/writers; avoid `read_to_end` on large data.
- Handle cancellation (Ctrl-C) via `ctrlc` or `tokio::signal`.
- Avoid blocking in async contexts; use appropriate runtimes only when necessary.

## Cross-Platform
- Test on macOS, Linux, Windows.
- Use `camino::Utf8PathBuf` for consistent path handling; convert at boundaries.
- Mind case sensitivity, path lengths on Windows, and newline differences.
- Provide shell completions for bash, zsh, fish, powershell.

## Security & Supply Chain
- Commit `Cargo.lock` for binaries.
- Run `cargo deny check` and `cargo audit` in CI.
- Avoid `unsafe` unless absolutely necessary; document and encapsulate it.
- Prefer least-privilege file permissions; use `tempfile` for temp work.
- Sanitize external inputs and paths; avoid command injection (use `std::process::Command` safely).

## Documentation
- `README.md` with quickstart, examples, exit codes, environment vars, config file format, and completion installation.
- `--help` should stand alone; include examples per subcommand.
- Changelog maintained with Conventional Commits and `CHANGELOG.md`.

## Versioning & Releases (Publishing)
- SemVer strictly. Automate with `cargo-release`.
- Build signed, reproducible artifacts for macOS (universal if feasible), Linux (gnu+musl), and Windows (msvc).
- Generate:
  - Tarballs/zip per target with binary, LICENSE, README, completions, and man page.
  - Homebrew formula, Scoop manifest, and optionally winget.
- Use `cargo-dist` or `actions-rs` based workflows to produce and upload releases to GitHub.
- Generate shell completions and man pages at build or release time, not at runtime.
- Optionally support self-update with `self_update` (opt-in; verify signatures).

## CI Pipeline (GitHub Actions suggested)
- Jobs: lint (fmt, clippy), test (unit+integration), deny/audit, build-matrix, release.
- Cache cargo registry/build. Fail on warnings in CI.
- Artefacts: attach binaries and checksums; optionally sign with `cosign`.

---

## Scaffold Templates

### Cargo.toml (binary + library)
```toml
[package]
name = "fast-tts-cli"
version = "0.1.0"
edition = "2021"
authors = ["Your Name <you@example.com>"]
description = "Fast text-to-speech CLI"
license = "MIT OR Apache-2.0"
readme = "README.md"
repository = "https://github.com/your/repo"
categories = ["command-line-utilities", "audio"]
keywords = ["cli", "tts", "audio"]

[dependencies]
anyhow = "1"
thiserror = "1"
clap = { version = "4", features = ["derive", "env"] }
clap_complete = "4"
clap_mangen = "0.2"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "fmt", "json"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
config = "0.14"
directories = "5"
indicatif = "0.17"
humantime = "2"
camino = "1"
fs-err = "2"
tempfile = "3"
which = "6"

[build-dependencies]
clap_mangen = "0.2"
clap_complete = "4"

[dev-dependencies]
assert_cmd = "2"
predicates = "3"
insta = { version = "1", features = ["json"] }

[profile.release]
# Optimize for size and speed; adjust as needed
lto = true
codegen-units = 1
opt-level = 3
strip = "symbols"

[features]
default = []

```

### src/main.rs
```rust
use anyhow::Result;

fn main() -> Result<()> {
    fast_tts_cli::run()
}
```

### src/lib.rs
```rust
pub mod cli;

use anyhow::Result;
use tracing_subscriber::{fmt, EnvFilter};

pub fn run() -> Result<()> {
    let cli = cli::Cli::parse();

    // Initialize logging
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let use_json = cli.log_format.as_deref() == Some("json");
    let subscriber = fmt()
        .with_env_filter(env_filter)
        .with_ansi(!use_json)
        .json(use_json)
        .finish();
    tracing::subscriber::set_global_default(subscriber).ok();

    match &cli.command {
        cli::Command::Example { name } => {
            tracing::info!(%name, "running example");
            println!("Hello, {}!", name);
        }
    }

    Ok(())
}
```

### src/cli.rs
```rust
use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser, Debug)]
#[command(name = "fast-tts", version, about = "Fast TTS CLI", long_about = None)]
pub struct Cli {
    /// Increase verbosity (-v, -vv)
    #[arg(short, long, action = clap::ArgAction::Count)]
    pub verbose: u8,

    /// Reduce output
    #[arg(short, long, action = clap::ArgAction::Count)]
    pub quiet: u8,

    /// Log format
    #[arg(long, value_enum)]
    pub log_format: Option<LogFormat>,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Copy, Clone, Debug, ValueEnum)]
pub enum LogFormat { Plain, Json }

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Example subcommand
    Example { name: String },
}

impl Cli {
    pub fn parse() -> Self { <Self as Parser>::parse() }
}
```

### tests/cli.rs
```rust
use assert_cmd::prelude::*;
use predicates::prelude::*;
use std::process::Command;

#[test]
fn prints_help() {
    let mut cmd = Command::cargo_bin("fast-tts-cli").unwrap();
    cmd.arg("--help");
    cmd.assert().success().stdout(predicate::str::contains("USAGE"));
}
```

### build.rs (generate man page and completions at build time)
```rust
use std::{env, fs, path::PathBuf};

fn main() {
    // Generate man and completions only in release builds for speed
    if env::var("PROFILE").as_deref() != Ok("release") {
        return;
    }

    use clap::CommandFactory;
    use clap_complete::{generate_to, shells::{Bash, Zsh, Fish, PowerShell}};
    use clap_mangen::Man;

    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    let mut cmd = fast_tts_cli::cli::Cli::command();

    // Completions
    let _ = generate_to(Bash, &mut cmd, "fast-tts", out.join("completions"));
    let _ = generate_to(Zsh, &mut cmd, "fast-tts", out.join("completions"));
    let _ = generate_to(Fish, &mut cmd, "fast-tts", out.join("completions"));
    let _ = generate_to(PowerShell, &mut cmd, "fast-tts", out.join("completions"));

    // Man page
    let man = Man::new(cmd);
    let mut buf = Vec::new();
    man.render(&mut buf).unwrap();
    fs::create_dir_all(out.join("man")).ok();
    fs::write(out.join("man").join("fast-tts.1"), buf).unwrap();
}
```

---

## GitHub Actions Hints
- Lint and Test:
```yaml
name: ci
on: [push, pull_request]
jobs:
  lint-test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - run: cargo fmt --all --check
      - run: cargo clippy --all-targets --all-features -- -D warnings
      - run: cargo deny check
      - run: cargo test --all-features --all-targets
```
- Release (matrix build and upload artifacts) via `cargo-dist` or a custom workflow. Ensure artifacts contain binary, LICENSE, README, completions, and man pages.

## Local Developer Tooling
- `rust-toolchain.toml` pin to `stable` with components `rustfmt`, `clippy`.
- Pre-commit: format, clippy, tests.
- `justfile` or `Makefile` tasks for build, test, lint, release.

---

## Definition of Done (for publishable CLI)
- All commands have helpful `--help` with examples.
- `README.md` documents features, config, env vars, exit codes, and completions.
- CI: fmt, clippy (no warnings), tests, deny/audit all green.
- Reproducible cross-platform release artifacts generated and attached to a GitHub Release.
- Version bumped and tagged; changelog updated.
- Basic telemetry either absent or opt-in with clear documentation.

Follow this rule strictly. If trade-offs are needed, document decisions in PRs and keep the UX consistent.