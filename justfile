set shell := ["bash", "-cu"]

# All quality gates — must be green before every commit (see CLAUDE.md).
check: check-generated check-rust check-shell check-py lines

check-generated:
    # The format48 source is author-local; CI checks every reproducible artifact.
    scripts/gen-kit-classes.py --check-local
    scripts/gen-google-fonts.py --check
    scripts/gen-installer-assets.py --check

check-rust:
    cargo fmt --all --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo test --workspace
    cargo clippy -p smabar --features no-self-update --all-targets --locked -- -D warnings
    cargo test -p smabar --features no-self-update --locked

check-shell:
    cd shell && bunx tsc --noEmit
    cd shell && bunx eslint . --max-warnings 0
    cd shell && bunx prettier --check .
    cd shell && bunx vitest run

check-py:
    python3 -m unittest discover -s scripts -p 'test_*.py'
    cd sdk/python && uv run ruff check
    cd sdk/python && uv run ruff format --check
    cd sdk/python && uv run mypy .
    cd sdk/python && uv run pytest -q

# 500-line limit for smabar source files (plugins exempt).
lines:
    scripts/check-file-length.sh

# Release bundles for the host platform (deb/rpm on Linux, NSIS on Windows,
# DMG plus the updater archive on macOS); output in target/release/bundle/.
build:
    scripts/fetch-uv.sh "$(rustc -vV | sed -n 's/^host: //p')"
    cd shell && bun run build:app
    if [[ "$(uname -s)" == Darwin ]]; then scripts/app-archive-macos.sh; fi

fmt:
    cargo fmt --all
    cd shell && bunx prettier --write .
    cd sdk/python && uv run ruff format

# Website snapshots are separate from app builds; never require a sibling repo in CI.
plugin-docs WEBSITE="../website":
    python3 scripts/export-plugin-docs.py "{{WEBSITE}}"

plugin-docs-check WEBSITE="../website":
    python3 scripts/export-plugin-docs.py "{{WEBSITE}}" --check

# Run the app against the Vite dev server. Always goes through scripts/dev.sh,
# which kills stray instances and refuses to start a second one.
dev:
    scripts/dev.sh

# Local update store on 127.0.0.1:8787 (ADR 0009); dev.sh's update check
# points here. Edit .dev-store/updates/latest.json to fake a release.
dev-store:
    scripts/dev-store.sh

# The local update pipeline (ADR 0009): builds the host packages, signs them
# with ~/.tauri/smabar.key and announces them in the dev store as VERSION — an
# override, so the running dev build (same Cargo version) sees a release.
dev-store-release VERSION: build
    TAURI_SIGNING_PRIVATE_KEY_PATH="${TAURI_SIGNING_PRIVATE_KEY_PATH:-$HOME/.tauri/smabar.key}" \
      scripts/release.py --version "{{VERSION}}" --artifacts target/release/bundle \
      --out .dev-store/updates --base-url http://127.0.0.1:8787/updates
