# Default: list available recipes
default:
    @just --list

# ── Setup ─────────────────────────────────────────────────────────────────────

# Full initial setup (install all dependencies)
setup: install

# Install Rust + Node dependencies and tools
install:
    @echo "==> Checking Rust toolchain..."
    rustup show
    @echo "==> Fetching Rust crate dependencies..."
    cargo fetch
    @echo "==> Installing cargo-nextest..."
    @cargo nextest --version >/dev/null 2>&1 || cargo install cargo-nextest --locked
    @echo "==> Installing Node dependencies..."
    pnpm install
    @echo "Done. Run 'just dev' to start the development servers."

# ── Build ──────────────────────────────────────────────────────────────────────

# Build everything (debug)
build: build-backend build-frontend

# Build the Rust backend (debug)
build-backend:
    cargo build 2>&1 | grep -E "^(error|warning\[)" || true

# Build the frontend
build-frontend:
    pnpm build

# ── Check / Lint ───────────────────────────────────────────────────────────────

# Run cargo check (fast type-check)
check:
    cargo check 2>&1 | grep -E "^(error|warning\[)" || true

# Lint everything
lint: lint-backend lint-frontend

# Run clippy
lint-backend:
    cargo clippy --workspace

# Run ESLint / Biome
lint-frontend:
    pnpm lint

# Validate inter-crate dependency rules (see internal-docs/backend-architecture.md)
check-deps:
    python3 scripts/check-deps.py

# DRY up workspace Cargo.toml manifests by inheriting shared deps from workspace root
autoinherit:
    @cargo autoinherit --version >/dev/null 2>&1 || cargo install cargo-autoinherit
    cargo autoinherit

# Format all code (clippy auto-fix + rustfmt + frontend)
fmt:
    cargo clippy --fix --allow-dirty --allow-staged --broken-code --workspace --lib && cargo fmt --all
    pnpm --filter oxy-web run format

# Check formatting without writing
fmt-check:
    cargo fmt --check
    pnpm --filter oxy-web run format:check

# ── Test ───────────────────────────────────────────────────────────────────────

# Run all tests with nextest
test:
    cargo nextest run

# Remove dangling test containers (Postgres/ClickHouse/MySQL) left by test runs
clean-test-containers:
    @docker ps -aq --filter "label=org.testcontainers.managed-by=testcontainers" | xargs -r docker rm -f

# ── Dev servers ────────────────────────────────────────────────────────────────

# Print instructions for starting backend + frontend dev servers
dev:
    @echo "Run in separate terminals:"
    @echo "  just dev-backend"
    @echo "  just dev-frontend"

# Start the Rust API server (http://localhost:3000)
dev-backend:
    cargo run start

# Start the Vite dev server (http://localhost:5173)
dev-frontend:
    pnpm run dev

# ── Database / Seed ────────────────────────────────────────────────────────────

# Seed the database with test users
seed:
    cargo run -- seed users

# Clear all seeded test data
seed-clear:
    cargo run -- seed clear

# Run database migrations manually
migrate:
    cargo run --bin migration

# ── Customer-apps publish smoke test ───────────────────────────────────────────
#
# End-to-end smoke for the self-serve publish pipeline against a local
# `oxy serve` using the FILESYSTEM build store (no S3/MinIO). Publishes a
# pre-built bundle via `oxy publish`, then fetches the served URL and checks
# HTML came back. Exit 0 = green; any failure aborts immediately.
#
# Usage:
#     just test-customer-apps-publish <org> <slug> <bundle-dir> <project-id>
#
# Example:
#     just test-customer-apps-publish test hello-oxy \
#         /Users/luong/oxy-hq/customer-apps/examples/hello-oxy/out \
#         cdba75a2-c074-4dfa-a77c-a505b2845944
#
# Prerequisites (one-time):
#   - `cargo build -p oxy-app`  (provides ./target/debug/oxy)
#   - A local `oxy serve` running WITHOUT OXY_CUSTOMER_APPS_S3_BUCKET set, so
#     builds land in the state dir, e.g.:
#         OXY_STATE_DIR="$HOME/.local/share/oxy" ./target/debug/oxy serve
#   - Logged in as an app-admin: `./target/debug/oxy login --env local`
#     (or export OXY_TOKEN=<app-admin api key>)
#   - A real project UUID in your local oxy DB (passed as <project-id> — the
#     app may be new, so build-config can't resolve it yet)
#
# Full background: internal-docs/customer-apps.md §3d + the self-serve design.
test-customer-apps-publish org slug bundle project_id:
    #!/usr/bin/env bash
    set -euo pipefail

    TARGET="${OXY_TARGET:-http://localhost:3000}"
    CREDS="${XDG_CONFIG_HOME:-$HOME/.config}/oxy/credentials.json"

    if [ ! -f "{{bundle}}/index.html" ]; then
        echo "ERROR: {{bundle}}/index.html missing — every bundle must have one" >&2
        exit 1
    fi
    if [ ! -x "./target/debug/oxy" ]; then
        echo "ERROR: ./target/debug/oxy missing — run 'cargo build -p oxy-app' first" >&2
        exit 1
    fi

    # Token: OXY_TOKEN, else the `oxy login` cache for this host.
    HOSTKEY="$(echo "$TARGET" | sed -E 's#^https?://##; s#/$##')"
    TOKEN="${OXY_TOKEN:-$(jq -r --arg h "$HOSTKEY" '.[$h].token // empty' "$CREDS" 2>/dev/null || true)}"
    if [ -z "$TOKEN" ]; then
        echo "ERROR: not authenticated for $TARGET." >&2
        echo "       Run: ./target/debug/oxy login --env local   (or export OXY_TOKEN=<app-admin key>)" >&2
        exit 1
    fi

    # Serve must be up (FS backend = OXY_CUSTOMER_APPS_S3_BUCKET unset in serve's env).
    if ! curl -sf "$TARGET/api/health" >/dev/null 2>&1 && ! curl -sf "$TARGET/health" >/dev/null 2>&1; then
        echo "ERROR: no oxy serve responding at $TARGET." >&2
        echo "       Start it: OXY_STATE_DIR=\"\$HOME/.local/share/oxy\" ./target/debug/oxy serve" >&2
        exit 1
    fi

    echo "==> [1/2] Publishing {{org}}/{{slug}} → $TARGET (filesystem build store, live channel)…"
    ./target/debug/oxy publish \
        --target "$TARGET" \
        --org {{org}} \
        --app {{slug}} \
        --project {{project_id}} \
        --dir "{{bundle}}" \
        --promote

    echo "==> [2/2] Fetching the served bundle…"
    URL="$TARGET/customer-apps/{{org}}/{{slug}}/"
    BODY="$(curl -sf -H "Authorization: Bearer $TOKEN" "$URL")"
    if ! echo "$BODY" | grep -qiE "<html|<!doctype|__OXY_APP__"; then
        echo "ERROR: $URL did not return recognizable HTML" >&2
        exit 1
    fi

    echo
    echo "✓ Smoke OK — published to the state dir and served from $URL"

# ── Release ────────────────────────────────────────────────────────────────────

# Preview the next release version and unreleased changelog (no side effects).
release-preview:
    @echo "==> Next version:"
    @uv run scripts/release/bump-version.py --dry-run
    @echo ""
    @echo "==> Unreleased changelog:"
    @git cliff --unreleased

# Dry-run: generate a combined changelog draft for one or more past releases.
# Example: just release-changelog-preview 0.5.34
# Example: just release-changelog-preview 0.5.33 0.5.34 0.5.35
release-changelog-preview +VERSIONS:
    uv run scripts/release/update-content-changelog.py --dry-run {{VERSIONS}}

# Manually trigger the release PR workflow on GitHub (requires gh CLI + auth).
release-trigger:
    gh workflow run prepare-release.yaml --ref main
