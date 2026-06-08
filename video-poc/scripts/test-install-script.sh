#!/usr/bin/env bash
#
# End-to-end smoke test for `edge/install-edge.sh` in a throwaway
# OrbStack Linux VM. Doubles as the per-box flow for the bearer →
# JWT migration described in `internal-docs/jwt-cutover.md` §2 —
# each re-onboarded box is one invocation of the install script,
# and this harness validates the path before you run it against a
# real customer install.
#
# What it does:
#   1. Spawns a temp HTTP server on the host serving this repo's
#      `video-poc/docker-compose.yml` + `video-poc/edge/oxy-edge.service`
#      (so the VM doesn't need GitHub access).
#   2. Creates a fresh Ubuntu VM in OrbStack (or reuses one).
#   3. Inside the VM, runs the local install script with `--compose-url`
#      and `--systemd-url` pointing at the host's temp server, plus the
#      device-id / device-secret / oxy-url you pass in.
#   4. Verifies the expected post-install state:
#      - /var/lib/oxy/device.json (0600)
#      - /opt/oxy-edge/{docker-compose.yml,.env}
#      - oxy-edge.service enabled + running
#      - Worker container alive
#      - Worker has talked to the control plane on JWT
#
# Usage:
#   scripts/test-install-script.sh \
#     --device-id 11111111-1111-1111-1111-111111111111 \
#     --device-secret <base64> \
#     --oxy-url http://host.orb.internal:3000
#
# Optional flags:
#   --vm-name <name>       VM to use (default: oxy-edge-test)
#   --reset                Delete + recreate the VM before the test
#   --keep                 Don't `docker compose down` the VM stack on exit
#
# Get device-id / device-secret from the Oxy UI's "Add device" wizard
# (or use the API; the wizard's copy-paste install snippet contains
# both values in plain text).
#
# Lima alternative (if you don't have OrbStack):
#   limactl start --name=oxy-edge-test template://docker
#   limactl shell oxy-edge-test
# then run the install command manually. The verifier section below
# works against any Linux host accessible via SSH; only the
# spawn-vm + remote-exec helpers below would need adjusting.

set -euo pipefail

VM_NAME="oxy-edge-test"
RESET=0
KEEP=0
DEVICE_ID=""
DEVICE_SECRET=""
OXY_URL=""
# Optional CLI override for the Tailscale auth key. Wins over the
# value sourced from `video-poc/.env` further down — same way
# DEVICE_ID/OXY_URL are. Empty means "fall through to the dotenv /
# host shell"; the install-edge.sh invocation below only passes
# `--ts-authkey` when something actually resolved.
TS_AUTHKEY_CLI=""
# Auto-mint: when set (or when DEVICE_ID/DEVICE_SECRET are unset),
# the script POSTs to `${OXY_URL}/<wsid>/fleet/prepared-devices` to
# get a fresh device identity instead of requiring the operator to
# run the UI wizard first. Defaults to off for backward-compat
# with explicit DEVICE_ID/DEVICE_SECRET callers.
MINT=0
# Workspace id used by the auto-mint call. Local serve mode uses
# the nil UUID (see `crates/app/src/server/serve_mode.rs`); cloud
# operators override via the env var or the `--workspace-id` flag.
WORKSPACE_ID="${WORKSPACE_ID:-00000000-0000-0000-0000-000000000000}"
SITE_ID="${SITE_ID:-}"
# Operator JWT for the auto-mint call when the backend has auth
# enabled (anything other than `oxy serve --local`). Grab it from
# the browser's localStorage `auth_token` key (DevTools →
# Application → Local Storage → http://localhost:5173). Passed
# verbatim as the `Authorization` header — the web app does the
# same and doesn't add a `Bearer ` prefix. Empty when running
# against `--local` (auth_enabled=false).
OXY_AUTH_TOKEN_CLI=""

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
EDGE_DIR="${PROJECT_DIR}/edge"
HTTP_PORT=18181

usage() {
  sed -n '2,/^$/p' "$0" | sed 's|^# \?||'
  exit "${1:-0}"
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --device-id)    DEVICE_ID="$2"; shift 2 ;;
    --device-secret) DEVICE_SECRET="$2"; shift 2 ;;
    --oxy-url)      OXY_URL="$2"; shift 2 ;;
    --vm-name)      VM_NAME="$2"; shift 2 ;;
    --ts-authkey)   TS_AUTHKEY_CLI="$2"; shift 2 ;;
    --mint)         MINT=1; shift ;;
    --workspace-id) WORKSPACE_ID="$2"; shift 2 ;;
    --site-id)      SITE_ID="$2"; shift 2 ;;
    --auth-token)   OXY_AUTH_TOKEN_CLI="$2"; shift 2 ;;
    --reset)        RESET=1; shift ;;
    --keep)         KEEP=1; shift ;;
    -h|--help)      usage 0 ;;
    *) echo "unknown arg: $1" >&2; usage 2 ;;
  esac
done

[[ -n "$OXY_URL" ]] || { echo "missing --oxy-url" >&2; usage 2; }

# Two-URL pattern: the mint call runs on the mac (host), the
# worker runs inside the OrbStack VM container. The two sides
# resolve hostnames very differently:
#
#   - From the mac:    `localhost` / `127.0.0.1` → backend ✓
#                       `host.orb.internal` → not resolvable ✗
#   - From the VM:     `host.orb.internal` → backend ✓
#                       `localhost` / `127.0.0.1` → container itself ✗
#
# So we normalize BOTH URLs regardless of which form the operator
# typed. The original `OXY_URL` stays untouched as a public/cloud
# URL when neither pattern matches — production deployments hit
# this path with `https://oxy.example.com/api` and both sides
# resolve it identically.
MINT_OXY_URL="$OXY_URL"   # mac-reachable
case "$OXY_URL" in
  *://host.orb.internal:*|*://host.orb.internal/*)
    MINT_OXY_URL="${OXY_URL//host.orb.internal/localhost}"
    echo "→ rewriting OXY_URL for mint call: ${OXY_URL} → ${MINT_OXY_URL}"
    echo "  (the mac doesn't resolve host.orb.internal; using localhost for the host-side call)"
    ;;
  *://localhost:*|*://localhost/*|*://127.0.0.1:*|*://127.0.0.1/*)
    OXY_URL_VM="${OXY_URL//localhost/host.orb.internal}"
    OXY_URL_VM="${OXY_URL_VM//127.0.0.1/host.orb.internal}"
    echo "→ rewriting OXY_URL for VM: ${OXY_URL} → ${OXY_URL_VM}"
    echo "  (worker runs inside the VM; localhost resolves to the container, not the host)"
    OXY_URL="$OXY_URL_VM"
    ;;
esac

# When DEVICE_ID/DEVICE_SECRET are unset we treat MINT as implied —
# the operator hasn't given us identity, so we have to fetch one
# from the control plane. Explicit `--mint` still works for re-mint
# scenarios where the operator passed stale credentials.
if [[ -z "$DEVICE_ID" || -z "$DEVICE_SECRET" ]]; then
  MINT=1
fi

# Source the host's project `.env` once at the top so values it
# carries (`TS_AUTHKEY`, `ANTHROPIC_API_KEY`, etc.) are visible to
# both the auto-mint call below AND the VM-side .env seeding much
# later. CLI flags + shell env override file contents.
SCRIPT_DIR_EARLY="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR_EARLY="$(cd "${SCRIPT_DIR_EARLY}/.." && pwd)"
HOST_PROJECT_DOTENV_EARLY="${PROJECT_DIR_EARLY}/.env"
if [[ -f "$HOST_PROJECT_DOTENV_EARLY" ]]; then
  set -a
  # shellcheck disable=SC1090
  source "$HOST_PROJECT_DOTENV_EARLY"
  set +a
fi

# Resolution: CLI flag > host shell / dotenv > empty. Done here
# (rather than further down) because the mint request below needs
# the value to include `ts_authkey` in the audit-logged install
# command.
HOST_TS_AUTHKEY="${TS_AUTHKEY_CLI:-${TS_AUTHKEY:-}}"

if [[ "$MINT" -eq 1 ]]; then
  echo "→ auto-minting device via ${MINT_OXY_URL}/${WORKSPACE_ID}/fleet/prepared-devices"

  # Auth resolution for the mint + sites calls. Same precedence as
  # everywhere else: CLI flag > host shell / dotenv > empty (local
  # serve mode). Without a token, calls work against `oxy serve
  # --local`; with cloud auth on they 401.
  HOST_OXY_AUTH_TOKEN="${OXY_AUTH_TOKEN_CLI:-${OXY_AUTH_TOKEN:-}}"
  auth_curl_args=()
  if [[ -n "$HOST_OXY_AUTH_TOKEN" ]]; then
    auth_curl_args=(-H "Authorization: ${HOST_OXY_AUTH_TOKEN}")
  fi

  api_call() {
    # Args: METHOD URL [extra_curl_args…]
    local method="$1"; shift
    local url="$1"; shift
    # `${arr[@]+"${arr[@]}"}` — expand only when set. Empty-array
    # expansion under `set -u` blows up on macOS's bash 3.2.
    curl -fsS -X "$method" ${auth_curl_args[@]+"${auth_curl_args[@]}"} "$@" "$url"
  }

  # Common 401 hint — used after every API call below so the
  # operator doesn't have to scroll back.
  auth_hint() {
    cat >&2 <<'HINT'
the backend returned 401. Options:
  (a) run the backend in zero-auth mode: `cargo run -p oxy-app -- serve --local`
  (b) grab your JWT from the browser (DevTools → Application → Local Storage →
      `auth_token`) and rerun with OXY_AUTH_TOKEN=<token> (or --auth-token <token>).
HINT
  }

  # Auto-pick a site when SITE_ID is unset. Cloud-mode operators
  # who run with --workspace-id should pass --site-id too; this
  # fallback is for local serve mode where there's typically one
  # site and asking is more friction than picking.
  if [[ -z "$SITE_ID" ]]; then
    sites_response=$(api_call GET "${MINT_OXY_URL}/${WORKSPACE_ID}/cameras/sites" 2>&1) || {
      echo "error: sites lookup failed:" >&2
      echo "$sites_response" >&2
      if [[ "$sites_response" == *"401"* ]]; then auth_hint; fi
      exit 1
    }
    SITE_ID=$(echo "$sites_response" \
      | python3 -c 'import json,sys; d=json.load(sys.stdin); print(d[0]["id"] if d else "")' \
      2>/dev/null || true)
    if [[ -z "$SITE_ID" ]]; then
      echo "error: no sites in workspace ${WORKSPACE_ID}; create one in the UI or pass --site-id" >&2
      exit 1
    fi
    echo "→ auto-picked first site: ${SITE_ID}"
  fi

  # Build the JSON body via python so we don't have to think about
  # quoting the (possibly null) auth key inside a heredoc. The
  # backend's PrepareDeviceBody accepts ts_authkey as
  # `Option<String>`; null/missing are equivalent.
  mint_body=$(python3 -c '
import json, sys, os
body = {"site_id": sys.argv[1], "hardware_label": "test-orbstack"}
key = os.environ.get("MINT_TS_AUTHKEY", "")
if key:
    body["ts_authkey"] = key
sys.stdout.write(json.dumps(body))
' "$SITE_ID" 2>/dev/null) || {
    echo "error: failed to build mint request body" >&2
    exit 1
  }
  mint_response=$(MINT_TS_AUTHKEY="$HOST_TS_AUTHKEY" \
    api_call POST "${MINT_OXY_URL}/${WORKSPACE_ID}/fleet/prepared-devices" \
      -H 'Content-Type: application/json' \
      --data-binary "$mint_body" 2>&1) || {
    echo "error: mint request failed:" >&2
    echo "$mint_response" >&2
    if [[ "$mint_response" == *"401"* ]]; then
      auth_hint
    else
      echo "(check OXY_URL is reachable + workspace_id is correct)" >&2
    fi
    exit 1
  }
  DEVICE_ID=$(echo "$mint_response" \
    | python3 -c 'import json,sys; print(json.load(sys.stdin)["device_id"])')
  DEVICE_SECRET=$(echo "$mint_response" \
    | python3 -c 'import json,sys; print(json.load(sys.stdin)["device_secret_b64"])')
  if [[ -z "$DEVICE_ID" || -z "$DEVICE_SECRET" ]]; then
    echo "error: mint response missing device_id or device_secret_b64:" >&2
    echo "$mint_response" >&2
    exit 1
  fi
  ts_hint=""
  [[ -n "$HOST_TS_AUTHKEY" ]] && ts_hint=" (with --ts-authkey baked into install_command)"
  echo "✓ minted device_id=${DEVICE_ID:0:8}…${ts_hint}"
fi

[[ -n "$DEVICE_ID" ]] || { echo "missing --device-id (auto-mint did not populate)" >&2; usage 2; }
[[ -n "$DEVICE_SECRET" ]] || { echo "missing --device-secret (auto-mint did not populate)" >&2; usage 2; }

command -v orb >/dev/null 2>&1 || {
  echo "error: orb (OrbStack CLI) not on PATH. Install: https://orbstack.dev/" >&2
  exit 1
}

# ─────────────────────────────────────────────────────────────────────
# Host HTTP server — serves compose + systemd unit to the VM
# ─────────────────────────────────────────────────────────────────────
#
# The install script wants to curl these. Pointing it at an https
# URL on github would skip our local edits, so we serve the in-repo
# files instead. Lives only for the duration of this test.

start_http_server() {
  local serve_dir="$1"
  # A previous run that crashed before its EXIT trap fired can leave a
  # python http.server holding the port. Reap it so we don't bomb with
  # EADDRINUSE — the traceback is opaque and the failure mode is
  # confusing during repeated --reset runs.
  local stale_pid
  stale_pid="$(lsof -nP -iTCP:"${HTTP_PORT}" -sTCP:LISTEN -t 2>/dev/null || true)"
  if [[ -n "$stale_pid" ]]; then
    echo "→ reaping stale http.server on :${HTTP_PORT} (pid ${stale_pid})"
    kill "$stale_pid" 2>/dev/null || true
    sleep 0.3
  fi
  echo "→ starting host HTTP server on :${HTTP_PORT} serving ${serve_dir}"
  ( cd "$serve_dir" && python3 -m http.server "$HTTP_PORT" >/tmp/oxy-install-test.http.log 2>&1 ) &
  HTTP_PID=$!
  # Wait until the server answers.
  for i in 1 2 3 4 5; do
    if curl -fsS "http://localhost:${HTTP_PORT}/" >/dev/null 2>&1; then
      echo "✓ HTTP server up (pid ${HTTP_PID})"
      return 0
    fi
    sleep 0.2
  done
  echo "error: HTTP server didn't come up — see /tmp/oxy-install-test.http.log" >&2
  exit 1
}

# A tiny tempdir that combines what the install script needs at the
# expected URL shapes. Symlinks keep us from copying.
SERVE_DIR="$(mktemp -d)"
ln -s "${PROJECT_DIR}/docker-compose.yml" "${SERVE_DIR}/docker-compose.yml"
ln -s "${EDGE_DIR}/oxy-edge.service" "${SERVE_DIR}/oxy-edge.service"
start_http_server "$SERVE_DIR"

cleanup() {
  if [[ -n "${HTTP_PID:-}" ]]; then
    kill "$HTTP_PID" 2>/dev/null || true
  fi
  rm -rf "$SERVE_DIR"
  if [[ "$KEEP" -eq 0 && -n "${HARNESS_VM_CREATED:-}" ]]; then
    echo "→ leaving VM ${VM_NAME} alive (use --reset on next run to recreate)"
  fi
}
trap cleanup EXIT

# Resolve a URL the VM can use to reach the host's HTTP server.
# OrbStack maps `host.orb.internal` to the host gateway — use that
# in the URLs we pass to the install script.
HOST_FROM_VM="host.orb.internal"
COMPOSE_URL="http://${HOST_FROM_VM}:${HTTP_PORT}/docker-compose.yml"
SYSTEMD_URL="http://${HOST_FROM_VM}:${HTTP_PORT}/oxy-edge.service"

# ─────────────────────────────────────────────────────────────────────
# VM lifecycle
# ─────────────────────────────────────────────────────────────────────

# OrbStack's `list` returns JSON we can grep ergonomically; the
# default text format also omits a header row, so a `grep -q` for
# the name suffices either way. Keep the JSON path since it's
# stable across CLI versions.
vm_exists() {
  orb list -f json 2>/dev/null \
    | python3 -c "
import json, sys
data = json.load(sys.stdin) if sys.stdin.readable() else []
sys.exit(0 if any(m.get('name') == '${VM_NAME}' for m in data) else 1)
"
}

if [[ "$RESET" -eq 1 ]] && vm_exists; then
  echo "→ resetting VM ${VM_NAME}"
  orb delete "$VM_NAME" >/dev/null
fi

if ! vm_exists; then
  echo "→ creating Ubuntu VM ${VM_NAME} (~10s)"
  # OrbStack CLI uses positional `DISTRO[:VERSION] [NAME]` —
  # there's no `-i` flag. The default user is your macOS user
  # mapped in; we explicitly run as root below via `-u root`.
  orb create ubuntu:22.04 "$VM_NAME" >/dev/null
  HARNESS_VM_CREATED=1
fi

# Run a command inside the VM as root. OrbStack's `orb run` defaults
# to the host user mapped into the guest; we want clean root for the
# install script.
vm_root() {
  # OrbStack's `orb run` parses `--` as an unknown flag rather than
  # the conventional end-of-flags marker; pass args directly.
  orb run -m "$VM_NAME" -u root "$@"
}

# Copy a file from the host into the VM by piping. The `orb push`
# command exists but for one-shot identity / install script we just
# stream via stdin.
vm_install_script() {
  orb run -m "$VM_NAME" -u root bash -c "cat > /tmp/install-edge.sh && chmod +x /tmp/install-edge.sh" \
    < "${EDGE_DIR}/install-edge.sh"
}

# ─────────────────────────────────────────────────────────────────────
# Install
# ─────────────────────────────────────────────────────────────────────

echo "→ preflight: ensuring curl + ca-certificates in VM"
vm_root apt-get update -qq
vm_root apt-get install -y -qq curl ca-certificates

echo "→ pushing local install-edge.sh into VM"
vm_install_script

# The compose file's `coturn:` service declares
# `${TURN_AUTH_SECRET:?…}` — the `:?` makes compose hard-fail when
# the value is missing, which trips systemd on the very first
# `compose pull` and the unit never settles. The install script
# preserves any existing `.env` content (only enforces OXY_URL),
# so we pre-seed a generated secret here. For a production
# install the operator would source TURN_AUTH_SECRET from the
# deployment's secret manager (and sync it to the backend's
# OXY_CAMERAS_TURN_AUTH_SECRET); for the smoke test we just need
# a value so compose validates.
# Carry-through for dev-loop env values that would otherwise be
# lost on every `cat >` run. Source the host's project `.env`
# first so a single canonical file (`video-poc/.env`) holds the
# secrets the operator keeps refreshing — TS_AUTHKEY,
# TURN_AUTH_SECRET, ANTHROPIC_API_KEY, etc. Shell env on the
# host wins over the file (so `FOO=bar make test-install` still
# overrides), and the dotenv values fill in everything that
# wasn't already exported.
#
# `set -a` makes every `key=value` line in the file an exported
# var; `set +a` restores the default. The early arg-parsing block
# above already sources this same file for the auto-mint path;
# re-sourcing here is idempotent and keeps this seeding section
# independently readable.
HOST_PROJECT_DOTENV="${PROJECT_DIR}/.env"
if [[ -f "$HOST_PROJECT_DOTENV" ]]; then
  echo "→ sourcing host ${HOST_PROJECT_DOTENV} for dev-loop secrets"
  set -a
  # shellcheck disable=SC1090
  source "$HOST_PROJECT_DOTENV"
  set +a
fi

# Generate a fresh TURN_AUTH_SECRET if neither the host shell nor
# the dotenv set one — keeps fresh-checkout installs working without
# the operator needing to seed it first. When a value IS set on the
# host, we propagate it verbatim so the backend's
# OXY_CAMERAS_TURN_AUTH_SECRET matches (both must agree for the
# WHEP auth callback to validate browser tokens).
HOST_TURN_SECRET="${TURN_AUTH_SECRET:-$(openssl rand -hex 32)}"
echo "→ seeding /opt/oxy-edge/.env"
vm_root mkdir -p /opt/oxy-edge
# Bake in the dev-loop env that `cat >` would otherwise wipe on
# every test-install run. The base install script only enforces
# OXY_URL, so anything we want persisted across resets has to
# land here:
#
#   TURN_AUTH_SECRET — compose declares it as `${VAR:?…}` so the
#     whole file fails validation when missing. Pulled from the
#     host dotenv/shell when set so the backend can decode WHEP
#     tokens minted by the browser; otherwise generated fresh.
#   TS_AUTHKEY — Tailscale auth key (tag:edge-box). Resolved from
#     the script's `--ts-authkey` CLI flag first (the prod-shaped
#     entry point the Makefile passes through), then the host
#     dotenv/shell. NOT written into the seeded .env here — the
#     install-edge.sh invocation below uses `--ts-authkey` which
#     mirrors what real customer installs do, so the test path
#     exercises the same code path as production.
#   COMPOSE_PROFILES — the video-poc compose puts every service
#     behind `profiles: ["edge"]`, but the systemd unit invokes
#     `docker compose up -d` without `--profile edge`. Setting
#     COMPOSE_PROFILES in .env is the documented way to default
#     the profile selection; equivalent to passing `--profile edge`.
#   ANTHROPIC_API_KEY — pass-through. If unset on the host, the
#     worker logs `edge.vlm_disabled` and skips compliance reports.
#   EDGE_FUNNEL_HOSTNAME — intentionally NOT written here.
#     `tailscale-funnel-init` auto-discovers the hostname from
#     `Self.DNSName` after login and writes it to
#     `/var/lib/outbox/funnel.env`; the worker entrypoint sources
#     that. Pre-seeding a value used to cause a confusing trap
#     where the test VM kept reporting the FIRST hostname tailscale
#     assigned, even after a tailnet rename or a `-N` suffix from
#     an ephemeral-device collision (e.g. `video-poc-edge-5`).
#     If you really need to pin a hostname (running tailscale
#     outside this stack), set `EDGE_FUNNEL_HOSTNAME_OVERRIDE` in
#     compose's `.env` instead.
#   PPE_YOLO_MODEL / PPE_YOLO_CONF_THRESHOLD — pass-through; default
#     to the canonical trained checkpoint mounted by the `models`
#     symlink below.
PPE_YOLO_MODEL_DEFAULT="/app/models/food_ppe-trained.pt"
# HOST_TS_AUTHKEY was already resolved at the top of the script
# (so the auto-mint POST could include it). Re-using it here.
HOST_ANTHROPIC_KEY="${ANTHROPIC_API_KEY:-}"
HOST_FUNNEL_HOSTNAME_OVERRIDE="${EDGE_FUNNEL_HOSTNAME_OVERRIDE:-}"
HOST_PPE_MODEL="${PPE_YOLO_MODEL:-$PPE_YOLO_MODEL_DEFAULT}"
HOST_PPE_THRESHOLD="${PPE_YOLO_CONF_THRESHOLD:-0.10}"
# TS_AUTHKEY intentionally NOT written here — install-edge.sh
# below receives it via `--ts-authkey` and writes the line itself.
vm_root bash -c "cat > /opt/oxy-edge/.env" <<EOF
TURN_AUTH_SECRET=${HOST_TURN_SECRET}
COMPOSE_PROFILES=edge
ANTHROPIC_API_KEY=${HOST_ANTHROPIC_KEY}
EDGE_FUNNEL_HOSTNAME_OVERRIDE=${HOST_FUNNEL_HOSTNAME_OVERRIDE}
PPE_YOLO_MODEL=${HOST_PPE_MODEL}
PPE_YOLO_CONF_THRESHOLD=${HOST_PPE_THRESHOLD}
EOF

# The video-poc compose is the dev compose — its `edge:` and
# `update-agent:` services have `build: ./edge`, which means
# compose looks for `/opt/oxy-edge/edge/Dockerfile` relative to
# the compose's WorkingDirectory. In real customer installs the
# wizard hands over a production-shape compose that uses
# `image: ghcr.io/oxy-hq/oxy-edge:vX.Y.Z` and doesn't need a build
# context at all — that compose doesn't exist yet (TODO), so for
# this smoke test we symlink the build context out of the host.
# OrbStack auto-mounts the macOS filesystem at /mnt/mac/.
HOST_EDGE_DIR_IN_VM="/mnt/mac${EDGE_DIR}"
HOST_CENTRAL_DIR_IN_VM="/mnt/mac${PROJECT_DIR}/central"
HOST_MODELS_DIR_IN_VM="/mnt/mac${PROJECT_DIR}/models"
echo "→ symlinking dev-compose bind-mount paths from host:"
echo "    /opt/oxy-edge/edge    -> ${HOST_EDGE_DIR_IN_VM}     (build context for edge/update-agent)"
echo "    /opt/oxy-edge/central -> ${HOST_CENTRAL_DIR_IN_VM}  (mediamtx.yml + samples + tailscale/funnel-init.sh)"
echo "    /opt/oxy-edge/models  -> ${HOST_MODELS_DIR_IN_VM}   (PPE_YOLO_MODEL weights; auto-picks up host retrains)"
vm_root ln -sfn "$HOST_EDGE_DIR_IN_VM" /opt/oxy-edge/edge
vm_root ln -sfn "$HOST_CENTRAL_DIR_IN_VM" /opt/oxy-edge/central
# `ln -sfn` won't replace a directory that docker compose may have
# auto-created on a prior run — clear that first so the symlink wins.
vm_root bash -c 'if [[ -d /opt/oxy-edge/models && ! -L /opt/oxy-edge/models ]]; then rmdir /opt/oxy-edge/models 2>/dev/null || rm -rf /opt/oxy-edge/models; fi'
vm_root ln -sfn "$HOST_MODELS_DIR_IN_VM" /opt/oxy-edge/models

echo "→ running install (this will install docker if missing — first run takes ~60s)"
# Conditionally pass `--ts-authkey` so the test exercises the same
# flag a real customer install will use. When unset, the tailscale
# sidecar stays "Logged out" — same end-state as the prod path
# without an auth key, so the smoke still catches misconfig
# downstream of TS_AUTHKEY.
if [[ -n "$HOST_TS_AUTHKEY" ]]; then
  echo "→ passing --ts-authkey (prefix ${HOST_TS_AUTHKEY:0:12}…)"
  TS_AUTHKEY_ARGS=(--ts-authkey "$HOST_TS_AUTHKEY")
else
  echo "→ no TS_AUTHKEY — tailscale sidecar will boot Logged out (live preview disabled)"
  TS_AUTHKEY_ARGS=()
fi
vm_root bash /tmp/install-edge.sh \
  --device-id "$DEVICE_ID" \
  --device-secret "$DEVICE_SECRET" \
  --oxy-url "$OXY_URL" \
  --compose-url "$COMPOSE_URL" \
  --systemd-url "$SYSTEMD_URL" \
  --hardware-label "test-orbstack" \
  --turn-auth-secret "$HOST_TURN_SECRET" \
  ${TS_AUTHKEY_ARGS[@]+"${TS_AUTHKEY_ARGS[@]}"} \
  --force \
  2>&1 | sed 's/^/  [install] /'

# ─────────────────────────────────────────────────────────────────────
# Post-install verification
# ─────────────────────────────────────────────────────────────────────

echo
echo "─── post-install verification ───"

pass() { echo "  ✓ $1"; }
fail() { echo "  ✗ $1" >&2; FAILED=1; }
FAILED=0

# Identity file
if vm_root test -f /var/lib/oxy/device.json; then
  perms=$(vm_root stat -c '%a' /var/lib/oxy/device.json)
  if [[ "$perms" == "600" ]]; then
    pass "device.json present at 0600"
  else
    fail "device.json perms ${perms}, expected 600"
  fi
else
  fail "device.json missing"
fi

# Compose + .env
if vm_root test -f /opt/oxy-edge/docker-compose.yml; then
  pass "docker-compose.yml installed"
else
  fail "docker-compose.yml missing"
fi
if vm_root test -f /opt/oxy-edge/.env; then
  if vm_root grep -q "^OXY_URL=${OXY_URL}\$" /opt/oxy-edge/.env; then
    pass ".env contains OXY_URL=${OXY_URL}"
  else
    fail ".env missing or wrong OXY_URL"
  fi
else
  fail ".env missing"
fi

# Systemd
if vm_root systemctl is-enabled --quiet oxy-edge.service; then
  pass "oxy-edge.service enabled"
else
  fail "oxy-edge.service not enabled"
fi
if vm_root systemctl is-active --quiet oxy-edge.service; then
  pass "oxy-edge.service active"
else
  fail "oxy-edge.service not active"
fi

# Containers
sleep 5  # give compose a beat
if vm_root bash -c "cd /opt/oxy-edge && docker compose ps --status running --format '{{.Service}}' | grep -q '^edge\$'"; then
  pass "edge worker container running"
else
  fail "edge worker container not running — check 'orb run -m ${VM_NAME} -u root docker compose -f /opt/oxy-edge/docker-compose.yml logs edge'"
fi

# JWT vs bearer — proves the migration actually flipped. The worker
# emits `{"msg":"edge.auth_mode","mode":"jwt"|"bearer", …}` on every
# startup (see worker/main.py). Parse the JSON line directly so a
# field-order shuffle in the future doesn't break this check.
echo "→ waiting up to 30s for worker to log edge.auth_mode…"
auth_observed=""
not_claimed_streak=0
for i in $(seq 1 30); do
  # Parse the most recent `edge.auth_mode` AND the most recent
  # `provision.waiting_for_claim`. The latter is the signal that
  # the device_id has no pending_claim server-side — bootstrap
  # can't proceed, so auth_mode is never logged. Detecting this
  # state early turns a 30s "no log line" timeout into a one-line
  # diagnostic with a clear recovery path.
  read -r auth_observed waiting_state < <(vm_root bash -c "
    cd /opt/oxy-edge && \
    docker compose logs --tail=300 edge 2>/dev/null | \
    python3 -c '
import json, sys
mode = \"\"
waiting = \"\"
for line in sys.stdin:
    line = line[line.find(\"{\") :] if \"{\" in line else \"\"
    if not line: continue
    try: rec = json.loads(line.rstrip())
    except Exception: continue
    msg = rec.get(\"msg\", \"\")
    if msg == \"edge.auth_mode\":
        mode = rec.get(\"mode\", \"\")
    elif msg == \"provision.waiting_for_claim\":
        waiting = rec.get(\"state\", \"\")
print(mode or \"-\", waiting or \"-\")
'
  " 2>/dev/null || echo "- -")
  if [[ "$auth_observed" == "-" ]]; then auth_observed=""; fi
  if [[ "$waiting_state" == "-" ]]; then waiting_state=""; fi

  if [[ -n "$auth_observed" ]]; then break; fi

  # Fail-fast when the worker is stuck on `not_claimed`. The
  # bootstrap can't succeed without a matching pending_claim, so
  # waiting longer won't help. 3 consecutive observations is enough
  # to rule out a transient timing race (the announce loop polls
  # every ~30s; this loop polls every 1s, so the same log line
  # appears multiple times before a real state change could land).
  if [[ "$waiting_state" == "not_claimed" ]]; then
    not_claimed_streak=$((not_claimed_streak + 1))
    if [[ "$not_claimed_streak" -ge 3 ]]; then
      fail "device_id has no pending claim on the backend — the worker is announcing but the server doesn't know about this device. Mint a fresh pair via 'Add device' in the UI and re-run with the new DEVICE_ID/DEVICE_SECRET (or set MINT=1 to auto-mint)."
      break
    fi
  else
    not_claimed_streak=0
  fi
  sleep 1
done

# If the streak-break above already failed, skip the case block —
# it would otherwise stack a redundant "no edge.auth_mode" line.
if [[ "$not_claimed_streak" -lt 3 ]]; then
  case "$auth_observed" in
    jwt)
      pass "worker authenticated with JWT (bearer → JWT cutover confirmed)"
      ;;
    bearer)
      fail "worker fell back to bearer — JWT mint failed; check device.json + secret"
      ;;
    "")
      fail "no edge.auth_mode log line after 30s — check worker logs"
      ;;
    *)
      fail "unexpected auth_mode: ${auth_observed}"
      ;;
  esac
fi

echo
if [[ "$FAILED" -eq 0 ]]; then
  echo "✓ all checks passed"
  echo
  echo "Next: confirm the box appears in your Oxy UI at ${OXY_URL}/ide/edge/list"
  echo "  Banner 'still on legacy bearer' count should be one lower."
  exit 0
else
  echo "✗ verification failed — inspect /opt/oxy-edge inside the VM:"
  echo "    orb run -m ${VM_NAME} -u root bash"
  exit 1
fi
