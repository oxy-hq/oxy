.PHONY: help setup install build check test lint fmt clean dev dev-backend dev-frontend seed

# Default target
help:
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-20s\033[0m %s\n", $$1, $$2}'
	@echo ""
	@echo "Usage: make <target>"

# ── Setup ─────────────────────────────────────────────────────────────────────

setup: install ## Full initial setup (install all dependencies)

install: ## Install Rust + Node dependencies and tools
	@echo "==> Checking Rust toolchain..."
	rustup show
	@echo "==> Fetching Rust crate dependencies..."
	cargo fetch
	@echo "==> Installing cargo-nextest..."
	@cargo nextest --version >/dev/null 2>&1 || cargo install cargo-nextest --locked
	@echo "==> Installing Node dependencies..."
	pnpm install
	@echo "Done. Run 'make dev' to start the development servers."

# ── Build ──────────────────────────────────────────────────────────────────────

build: build-backend build-frontend ## Build everything (debug)

build-backend: ## Build the Rust backend (debug)
	cargo build | grep -E "^(error|warning\[)" || true

build-frontend: ## Build the frontend
	pnpm build

# ── Check / Lint ───────────────────────────────────────────────────────────────

check: ## Run cargo check (fast type-check)
	cargo check | grep -E "^(error|warning\[)" || true

lint: lint-backend lint-frontend ## Lint everything

lint-backend: ## Run clippy
	cargo clippy --workspace

lint-frontend: ## Run ESLint / Biome
	pnpm lint

fmt: ## Format all code (clippy auto-fix + rustfmt + frontend)
	cargo clippy --fix --allow-dirty --allow-staged --broken-code --workspace --lib && cargo fmt --all
	pnpm --filter oxy-web run format

fmt-check: ## Check formatting without writing
	cargo fmt --check
	pnpm format:docs:check

# ── Test ───────────────────────────────────────────────────────────────────────

test: ## Run all tests with nextest
	cargo nextest run

# ── Dev servers ────────────────────────────────────────────────────────────────

dev: ## Start backend + frontend dev servers (requires two terminals or a process manager)
	@echo "Run in separate terminals:"
	@echo "  make dev-backend"
	@echo "  make dev-frontend"

dev-backend: ## Start the Rust API server (https://localhost:3000)
	cargo run serve -- --http2-only

dev-frontend: ## Start the Vite dev server (http://localhost:5173)
	pnpm run dev

# ── Database / Seed ────────────────────────────────────────────────────────────

seed: ## Seed the database with test users
	cargo run -- seed users

seed-clear: ## Clear all seeded test data
	cargo run -- seed clear

migrate: ## Run database migrations manually
	cargo run --bin migration

