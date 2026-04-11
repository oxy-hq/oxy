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
    pnpm format:docs:check

# ── Test ───────────────────────────────────────────────────────────────────────

# Run all tests with nextest
test:
    cargo nextest run

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
