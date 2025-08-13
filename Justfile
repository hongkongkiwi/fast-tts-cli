# just: handy task runner. Install: `cargo install just`

set shell := ["bash", "-eu", "-o", "pipefail", "-c"]

# Default recipe
default: help

help:
	@echo "Available recipes:"
	@just --list

# Format, lint, build, test
check:
	cargo fmt --all
	cargo clippy --all-targets -- -D warnings
	cargo build --release --locked
	time cargo test --all --locked

# Bump version in Cargo.toml (example: just bump 0.1.1)
bump VERSION:
	sed -i '' -e "s/^version = \".*\"/version = \"{{VERSION}}\"/" Cargo.toml
	git add Cargo.toml
	git commit -m "chore: bump version to {{VERSION}}"

# Create a release tag and push, which triggers CI release
release VERSION:
	git tag v{{VERSION}}
	git push origin v{{VERSION}}

# Convenience for patch/minor/major using cargo-release style
release-patch:
	@ver=$(cargo metadata --no-deps --format-version=1 | jq -r '.packages[] | select(.name=="fast-tts-cli") | .version') && \
	new=$(python3 - <<'PY'
from packaging.version import Version
import os
v = Version(os.environ['ver'])
print(f"{v.major}.{v.minor}.{v.micro+1}")
PY
	) && just bump "$new" && just release "$new"

release-minor:
	@ver=$(cargo metadata --no-deps --format-version=1 | jq -r '.packages[] | select(.name=="fast-tts-cli") | .version') && \
	new=$(python3 - <<'PY'
from packaging.version import Version
import os
v = Version(os.environ['ver'])
print(f"{v.major}.{v.minor+1}.0")
PY
	) && just bump "$new" && just release "$new"

release-major:
	@ver=$(cargo metadata --no-deps --format-version=1 | jq -r '.packages[] | select(.name=="fast-tts-cli") | .version') && \
	new=$(python3 - <<'PY'
from packaging.version import Version
import os
v = Version(os.environ['ver'])
print(f"{v.major+1}.0.0")
PY
	) && just bump "$new" && just release "$new"
