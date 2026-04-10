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
    @echo "==> Installing release-plz (>= 0.3.151 required for git_only support)..."
    @just _ensure-release-plz
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

# Preview the next release: bumps Cargo.toml versions and updates CHANGELOG.md locally.
# No GitHub interaction — safe to run and inspect, then revert with: git checkout .
release-preview: _ensure-release-plz
    release-plz update --config release-plz.toml

# Install or upgrade release-plz to a version that supports git_only (>= 0.3.151).
_ensure-release-plz:
    @release-plz --version 2>/dev/null | grep -qE "0\.[0-9]+\.(15[1-9]|1[6-9][0-9]|[2-9][0-9]{2})" \
        || { echo "==> Upgrading release-plz (need >= 0.3.151)..."; cargo install release-plz --locked; }
