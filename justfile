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

# Start the OAuth bounce proxy (http://localhost:8429). Lets several local
# dev instances share one registered redirect URI per provider (Google + GitHub);
# it forwards each callback back to the instance that started the flow.
# See scripts/oauth-bounce.mjs.
oauth-proxy:
    node scripts/oauth-bounce.mjs

# ── Database / Seed ────────────────────────────────────────────────────────────

# Seed the demo project (guest user + Local org + nil-UUID workspace at ./examples).
seed:
    cargo run -- seed

# Drop the demo workspace row.
seed-clear:
    cargo run -- seed --clear

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

    if [ ! -f "{{ bundle }}/index.html" ]; then
        echo "ERROR: {{ bundle }}/index.html missing — every bundle must have one" >&2
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

    echo "==> [1/2] Publishing {{ org }}/{{ slug }} → $TARGET (filesystem build store, live channel)…"
    ./target/debug/oxy publish \
        --target "$TARGET" \
        --org {{ org }} \
        --app {{ slug }} \
        --project {{ project_id }} \
        --dir "{{ bundle }}" \
        --promote

    echo "==> [2/2] Fetching the served bundle…"
    URL="$TARGET/customer-apps/{{ org }}/{{ slug }}/"
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
    uv run scripts/release/update-content-changelog.py --dry-run {{ VERSIONS }}

# Manually trigger the release PR workflow on GitHub (requires gh CLI + auth).
release-trigger:
    gh workflow run prepare-release.yaml --ref main

# ── Airhouse local stack ───────────────────────────────────────────────────────

# Boot the local airhouse stack.
airhouse-up:
    docker compose -f docker-compose.airhouse.yml up -d
    @echo
    @echo "Next — streamlined (recommended), two commands:"
    @echo "  just airhouse-precompile   # migrate + seed + compile+PROMOTE the demo workspace"
    @echo "  just airhouse-fleet        # build + run ide :3000 + serve :3002 (env auto-set)"
    @echo "  # then: just routing-check 3002"
    @echo
    @echo "Or step-by-step:"
    @echo "  set -a; source .env.airhouse; set +a"
    @echo "  cargo run -p migration --bin migration                                     # one-shot"
    @echo "  cargo run -p oxy-app -- seed                                               # guest user + Local org + workspace at ./examples + OXY_GLOBAL_ADMINS as Owners"
    @echo "  # Compile+PROMOTE the seeded workspace. ABSOLUTE path: a RELATIVE --workspace-path"
    @echo "  # silently under-compiles (the discover globs miss → only config.yml). --promote sets"
    @echo "  # current_revision_id so the serve fleet can actually read it."
    @echo "  cargo run -p oxy-app -- compile --workspace-path $PWD/examples --workspace-id 70787bb2-e11b-5488-b2c3-02e60d5fc7d3 --enterprise --promote"
    @echo "  # Split fleet — run BOTH (the serve node self-proxies IdeOnly → the ide node)."
    @echo "  # The serve node's --internal-port 0 turns OFF its internal API (which also"
    @echo "  # defaults to 3001) so it can't clash with the ide node's internal API on :3001."
    @echo "  # OXY_INPROC_GLOBAL_WORKER=1 on the IDE node is REQUIRED: it runs the global"
    @echo "  # driver that DRAINS the compile queue. Without it, admin/auto compiles sit"
    @echo "  # 'queued' forever, no revision is produced, and the serve node 503s every"
    @echo "  # compiled read with needs_recompile. The serve node must NOT have it (it has"
    @echo "  # no working copy; --no-workers keeps it a pure reader). Mirrors oxy-dev, where"
    @echo "  # the StatefulSet sets OXY_INPROC_GLOBAL_WORKER=1 and the serve fleet strips it."
    @echo "  OXY_ROLE=ide   OXY_INPROC_GLOBAL_WORKER=1 cargo run -p oxy-app -- serve --enterprise  # ide node (full FS + drains compiles; main :3000, internal :3001)"
    @echo "  OXY_ROLE=serve OXY_IDE_UPSTREAM=http://localhost:3000 cargo run -p oxy-app -- serve --enterprise --no-workers --port 3002 --internal-port 0   # stateless serve node"
    @echo "  just routing-check 3002                                                   # probe serve → IdeOnly shows Forwarded-Via: serve + Served-By: ide"

# One-shot precompile: migrate + seed + compile+PROMOTE the demo workspace into
# Postgres so the stateless serve fleet can read it. Idempotent — re-run freely.
# Run after `just airhouse-up`. The workspace UUID is deterministic:
# Uuid::new_v5(NAMESPACE_DNS, "demo.oxy.local") = 70787bb2-… (seed.rs).
airhouse-precompile:
    #!/usr/bin/env bash
    set -euo pipefail
    set -a; source .env.airhouse; set +a
    echo "→ waiting for postgres"
    for _ in $(seq 1 60); do
      docker compose -f docker-compose.airhouse.yml exec -T airhouse-postgres \
        pg_isready -U airhouse -d oxydb >/dev/null 2>&1 && break || sleep 1
    done
    echo "→ migrate"; cargo run -p migration --bin migration
    echo "→ seed";    cargo run -p oxy-app -- seed
    echo "→ compile + promote (ABSOLUTE path — a relative --workspace-path under-compiles)"
    cargo run -p oxy-app -- compile --workspace-path "$PWD/examples" \
      --workspace-id 70787bb2-e11b-5488-b2c3-02e60d5fc7d3 --enterprise --promote --skip-migrations
    echo "✓ demo workspace compiled + promoted. Next: just airhouse-fleet"

# Build once, then run the split fleet (ide :3000 + serve :3002). The ide starts
# first and finishes migrating before serve starts (avoids a concurrent-migration
# race). OXY_INPROC_GLOBAL_WORKER on the ide node drains the compile queue; the
# serve node serves the latest compiled revision regardless of branch (no per-node
# default-branch config needed). Both nodes' logs stream here, prefixed
# [ide]/[serve] (also saved raw to /tmp/oxy-*.log). Ctrl-C stops both.
airhouse-fleet:
    #!/usr/bin/env bash
    set -euo pipefail
    set -a; source .env.airhouse; set +a
    echo "→ build"; cargo build -p oxy-app
    bin=./target/debug/oxy

    # Each node streams to THIS terminal (prefixed [ide]/[serve]) AND to a raw
    # /tmp log — so a startup crash (e.g. a failed migration) is visible here, not
    # hidden in a file you have to tail. The ide starts FIRST and we block until
    # it has migrated + bound :3000 before starting serve: launching both at once
    # ran DB migrations concurrently, and the CREATE TYPE enum migrations collided
    # (duplicate key on pg_type), crashing whichever node lost the race.
    echo "→ ide :3000 (owns migrations)"
    OXY_ROLE=ide OXY_INPROC_GLOBAL_WORKER=1 "$bin" serve --enterprise \
        > >(tee /tmp/oxy-ide.log | awk '{ print "[ide]   " $0; fflush() }') 2>&1 &
    ide=$!
    trap 'echo; echo stopping; kill $ide ${serve:-} 2>/dev/null || true' INT TERM EXIT

    echo "→ waiting for ide :3000 (migrations + bind)…"
    until curl -sf -m2 http://localhost:3000/health >/dev/null 2>&1; do
        kill -0 "$ide" 2>/dev/null || { echo "[fleet] ide exited during startup — see [ide] log above"; exit 1; }
        sleep 1
    done

    echo "→ serve :3002 (migrations already applied by ide)"
    OXY_ROLE=serve OXY_IDE_UPSTREAM=http://localhost:3000 "$bin" serve --enterprise --no-workers --port 3002 --internal-port 0 \
        > >(tee /tmp/oxy-serve.log | awk '{ print "[serve] " $0; fflush() }') 2>&1 &
    serve=$!

    echo "fleet up | logs streaming here (+ /tmp/oxy-{ide,serve}.log) | just routing-check 3002"
    wait

# Tear it down (drops volumes; pass --keep-data to retain).
airhouse-down *FLAGS:
    docker compose -f docker-compose.airhouse.yml down {{ if FLAGS =~ "--keep-data" { "" } else { "-v" } }}

# Tail logs; pass a service name to focus.
airhouse-logs *SERVICE:
    docker compose -f docker-compose.airhouse.yml logs -f {{ SERVICE }}

# Show services, buckets, and DBs.
airhouse-status:
    @docker compose -f docker-compose.airhouse.yml ps
    @echo
    @echo "==> Buckets on MinIO:"
    @docker compose -f docker-compose.airhouse.yml run --rm --no-deps -T --entrypoint sh airhouse-createbucket \
        -c 'mc alias set m http://airhouse-minio:9000 minioadmin minioadmin >/dev/null && mc ls m' \
        2>/dev/null || echo "  (minio not reachable yet)"
    @echo
    @echo "==> Databases on airhouse-postgres:"
    @docker compose -f docker-compose.airhouse.yml exec -T airhouse-postgres \
        psql -U airhouse -d airhouse -tc \
        "SELECT datname FROM pg_database WHERE datname IN ('airhouse','airhouse_cp','oxydb') ORDER BY 1" \
        2>/dev/null || echo "  (postgres not reachable yet)"

# psql shell on oxydb.
airhouse-psql:
    docker compose -f docker-compose.airhouse.yml exec airhouse-postgres \
        psql -U airhouse -d oxydb

# Print blob keys from PG + objects from MinIO.
airhouse-verify-blobs:
    @echo "==> semantic_views with blob keys:"
    @set -a; . ./.env.airhouse; set +a; \
        psql "$OXY_DATABASE_URL" -c \
        "SELECT name, substring(compiled_sql_blob_key, 1, 80) AS key FROM semantic_views WHERE compiled_sql_blob_key IS NOT NULL LIMIT 10;"
    @echo
    @echo "==> Objects in s3://oxy-compile-blobs/workspaces/:"
    @set -a; . ./.env.airhouse; set +a; \
        AWS_PAGER='' aws --endpoint-url "$AWS_ENDPOINT_URL" \
        s3 ls "s3://${OXY_COMPILE_BLOB_S3_BUCKET}/workspaces/" --recursive

# ── Routing boundary check ─────────────────────────────────────────────────────

# Probe a running node and show how each route ROLE is handled, by dumping the
# x-oxy-* headers (enforce_role stamps them before the handler, so even an
# unauth 401/421 reveals the routing). Run against:
#   the IDE node   (OXY_ROLE=ide,   :3000)  → everything Served-By: ide
#   the SERVE node (OXY_ROLE=serve, :3002)  → FleetOk Served-By: serve, and on
#       IdeOnly routes: Forwarded-Via: serve + Served-By: ide  (OXY_IDE_UPSTREAM set)
#       or 421 + Required-Role: ide                            (OXY_IDE_UPSTREAM unset)
# Default ws is the deterministic `oxy seed` demo workspace; any uuid works
# (classification is path-based, so the headers show even on a 404).
routing-check port="3000" ws="70787bb2-e11b-5488-b2c3-02e60d5fc7d3":
    #!/usr/bin/env bash
    set -eu
    base="http://localhost:{{ port }}"
    tally=$(mktemp)
    probe() {  # METHOD PATH
      local code by fwd
      code=$(curl -s -o /dev/null -w '%{http_code}' -D /tmp/_rch --max-time 5 -X "$1" "$base$2" || echo "---")
      by=$(grep -i '^x-oxy-served-by:' /tmp/_rch | sed 's/.*: *//; s/@.*//; s/\r//')
      grep -iq '^x-oxy-forwarded-via:' /tmp/_rch && fwd="  (serve→ide)" || fwd=""
      printf '  %-4s %-42s %-5s %s%s\n' "$1" "$2" "$code" "${by:-?}" "$fwd"
      echo "${by:-?}" >> "$tally"
    }
    echo "Routing check → $base  (ws={{ ws }})"
    echo "  401/200 is fine — x-oxy-served-by is stamped before auth; we're checking ROUTING."
    echo
    echo "FleetOk — no filesystem → the stateless serve fleet answers directly:"
    probe GET  /api/health
    probe GET  /api/{{ ws }}/threads
    probe GET  /api/{{ ws }}/apps
    probe GET  /api/{{ ws }}/agents
    probe GET  /api/{{ ws }}/databases
    probe GET  /api/{{ ws }}/tests
    probe GET  /api/{{ ws }}/traces
    probe GET  /api/{{ ws }}/semantic/monitors
    probe GET  /api/{{ ws }}/analytics/runs/r1/events
    probe GET  /api/{{ ws }}/blocks
    probe GET  /api/{{ ws }}/world-model/cameras
    probe GET  /api/{{ ws }}/results/files/abc.parquet
    echo
    echo "IdeOnly — genuinely needs the working copy / .git / node-local state → ide:"
    probe GET  /ide
    probe GET  /api/{{ ws }}/files
    probe POST /api/{{ ws }}/compile
    probe GET  /api/{{ ws }}/branches
    probe GET  /api/{{ ws }}/details
    probe GET  /api/{{ ws }}/status
    probe GET  /api/{{ ws }}/events
    probe GET  /api/{{ ws }}/world-model/events
    probe GET  /api/{{ ws }}/charts/x.json
    echo
    echo "── tally (this sample) ──  served-by serve: $(grep -c '^serve$' "$tally" || true)   ide: $(grep -c '^ide$' "$tally" || true)"
    echo "  (the FleetOk set is the BULK of the real API — apps/agents/semantic/analytics/orgs/users/billing/… all serve)"
    rm -f "$tally" /tmp/_rch
