#!/usr/bin/env bash
# Oxy edge box installer.
#
# Provisions a self-purchased COTS box (Jetson Orin Nano,
# Intel N100, generic x86 server, etc.) into a workspace's
# fleet in one command. Designed to be invoked from the
# "Add device" wizard's copy-paste snippet:
#
#   curl -sSL https://<your-oxy>/install-edge.sh \
#     | sudo bash -s -- \
#         --device-id <uuid> \
#         --device-secret <base64> \
#         --oxy-url https://<your-oxy>
#
# Or driven from a checked-out repo locally:
#
#   sudo ./install-edge.sh \
#       --device-id <uuid> \
#       --device-secret <base64> \
#       --oxy-url https://<your-oxy> \
#       --hardware-label "Jetson Orin Nano"
#
# What it does, in order:
#   1. Verify dependencies (curl, docker, docker compose v2).
#      Installs docker via the convenience script if missing.
#   2. Create /var/lib/oxy and write the device identity JSON
#      at 0600 perms.
#   3. Create /opt/oxy-edge, drop the docker-compose.yml and
#      the .env (with OXY_URL) into it.
#   4. Install /etc/systemd/system/oxy-edge.service and
#      `systemctl enable --now` it.
#   5. Tail the unit's journal for ~10s so the operator sees
#      the boot succeed without separately running journalctl.
#
# Idempotency: re-running with the same args overwrites
# device.json + .env + the unit file but leaves running
# containers alone (systemctl restart picks up the new
# compose). Re-running with DIFFERENT device-id rotates the
# identity — which is a destructive operation; the script
# refuses unless --force is passed.
#
# Exits non-zero on any step failure; safe for `set -e`
# parent contexts.

set -euo pipefail

# ── Defaults ─────────────────────────────────────────────────

INSTALL_ROOT="/opt/oxy-edge"
IDENTITY_DIR="/var/lib/oxy"
SERVICE_NAME="oxy-edge.service"
SYSTEMD_PATH="/etc/systemd/system/${SERVICE_NAME}"

# These two come from the same repo the install script does;
# the wizard's copy-paste hard-codes the right tag for the
# build the operator is on.
COMPOSE_URL_DEFAULT="https://raw.githubusercontent.com/oxy-hq/oxy/main/video-poc/docker-compose.yml"
SYSTEMD_URL_DEFAULT="https://raw.githubusercontent.com/oxy-hq/oxy/main/video-poc/edge/oxy-edge.service"
ENABLE_SUPPORT_URL_DEFAULT="https://raw.githubusercontent.com/oxy-hq/oxy/main/video-poc/edge/enable-remote-support.sh"
DISABLE_SUPPORT_URL_DEFAULT="https://raw.githubusercontent.com/oxy-hq/oxy/main/video-poc/edge/disable-remote-support.sh"

DEVICE_ID=""
DEVICE_SECRET=""
OXY_URL=""
HARDWARE_LABEL="unspecified"
SKU="self-installed"
COMPOSE_URL="${OXY_COMPOSE_URL:-${COMPOSE_URL_DEFAULT}}"
SYSTEMD_URL="${OXY_SYSTEMD_URL:-${SYSTEMD_URL_DEFAULT}}"
ENABLE_SUPPORT_URL="${OXY_ENABLE_SUPPORT_URL:-${ENABLE_SUPPORT_URL_DEFAULT}}"
DISABLE_SUPPORT_URL="${OXY_DISABLE_SUPPORT_URL:-${DISABLE_SUPPORT_URL_DEFAULT}}"
FORCE=0
SKIP_DOCKER_INSTALL=0
# Tailscale auth key — short-lived (`tskey-auth-…`), tagged
# `tag:edge-box`. When set, the tailscale sidecar logs in
# automatically on first boot and the `tailscale-funnel-init`
# service publishes MTX's WHEP port via Funnel. Live preview
# then works in the operator UI without any further hand-config.
# Without it, the rest of the pipeline (events, compliance
# reports, recordings) still works — only live preview is gated.
TS_AUTHKEY=""
# Beta-support escape hatch (default off — see
# `internal-docs/video-processing-fleet-architecture.md` § Remote
# access). When --allow-remote-support is set, the installer runs
# enable-remote-support.sh after the main service is up, which
# joins the host to the tailnet as tag:edge-box-host with
# Tailscale SSH on. Requires --ts-host-authkey (a key permitted
# to carry tag:edge-box-host). Without --allow-remote-support, no
# host SSH path is created at all.
ALLOW_REMOTE_SUPPORT=0
TS_HOST_AUTHKEY=""
# Rotation hygiene: on re-run, the previous TS_AUTHKEY stays in
# .env unless we either (a) pass a fresh --ts-authkey or (b) pass
# --clear-ts-authkey. Real-world flow: an operator rotated the
# key in the Tailscale admin and wants the box to forget the old
# one without yet having a replacement.
CLEAR_TS_AUTHKEY=0

# ── Output helpers ───────────────────────────────────────────

log() { printf '\033[1;34m[oxy-edge]\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33m[oxy-edge]\033[0m %s\n' "$*" >&2; }
die() {
    printf '\033[1;31m[oxy-edge]\033[0m %s\n' "$*" >&2
    exit 1
}

# ── Arg parsing ──────────────────────────────────────────────

usage() {
    cat <<EOF
Usage: $0 [options]

Required:
  --device-id <uuid>           Device UUID (from the Add device wizard)
  --device-secret <base64>     Device secret, base64-encoded (no padding)
  --oxy-url <url>              Control-plane URL (e.g. https://oxy.example.com)

Optional:
  --hardware-label <text>          Free-text label; stamped on device_registry
  --ts-authkey <tskey-auth-…>      Tailscale auth key (tagged tag:edge-box).
                                    Enables live preview without further setup.
  --clear-ts-authkey               Drop any TS_AUTHKEY currently in .env (without
                                    requiring a fresh --ts-authkey). Useful after
                                    rotating the key in the Tailscale admin so
                                    the previous value doesn't sit on disk.
  --allow-remote-support           Opt this box in to Oxy operator SSH-via-tailnet.
                                    Default OFF. When set, requires --ts-host-authkey.
                                    Can be enabled later with
                                    /opt/oxy-edge/enable-remote-support.sh.
  --ts-host-authkey <tskey-auth-…> Tailscale auth key permitted to carry
                                    tag:edge-box-host. Required with
                                    --allow-remote-support.
  --compose-url <url>              Override the docker-compose.yml source
  --systemd-url <url>              Override the systemd unit source
  --skip-docker-install            Don't install docker even if missing
  --force                          Overwrite a different existing device identity
  -h, --help                       Show this message
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --device-id) DEVICE_ID="$2"; shift 2 ;;
        --device-secret) DEVICE_SECRET="$2"; shift 2 ;;
        --oxy-url) OXY_URL="$2"; shift 2 ;;
        --hardware-label) HARDWARE_LABEL="$2"; shift 2 ;;
        --ts-authkey) TS_AUTHKEY="$2"; shift 2 ;;
        --clear-ts-authkey) CLEAR_TS_AUTHKEY=1; shift ;;
        --allow-remote-support) ALLOW_REMOTE_SUPPORT=1; shift ;;
        --ts-host-authkey) TS_HOST_AUTHKEY="$2"; shift 2 ;;
        --compose-url) COMPOSE_URL="$2"; shift 2 ;;
        --systemd-url) SYSTEMD_URL="$2"; shift 2 ;;
        --skip-docker-install) SKIP_DOCKER_INSTALL=1; shift ;;
        --force) FORCE=1; shift ;;
        -h|--help) usage; exit 0 ;;
        *) warn "unknown arg: $1"; usage; exit 2 ;;
    esac
done

[[ -n "$DEVICE_ID" ]] || die "missing --device-id"
[[ -n "$DEVICE_SECRET" ]] || die "missing --device-secret"
[[ -n "$OXY_URL" ]] || die "missing --oxy-url"

if [[ "$ALLOW_REMOTE_SUPPORT" -eq 1 && -z "$TS_HOST_AUTHKEY" ]]; then
    die "--allow-remote-support requires --ts-host-authkey (a key permitted to carry tag:edge-box-host)"
fi

if [[ "$(id -u)" -ne 0 ]]; then
    die "must run as root (sudo); the script writes to /var/lib/oxy, /opt/oxy-edge, and /etc/systemd/system"
fi

# Light validation — better to fail here than after we've
# half-installed things.
if ! [[ "$DEVICE_ID" =~ ^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$ ]]; then
    die "--device-id is not a valid UUID: $DEVICE_ID"
fi
if [[ "$OXY_URL" != http://* && "$OXY_URL" != https://* ]]; then
    die "--oxy-url must start with http:// or https://: $OXY_URL"
fi

# ── Step 1 — dependencies ────────────────────────────────────

log "checking dependencies"
command -v curl >/dev/null 2>&1 || die "curl is required (apt-get install curl)"

if ! command -v docker >/dev/null 2>&1; then
    if [[ "$SKIP_DOCKER_INSTALL" -eq 1 ]]; then
        die "docker not found and --skip-docker-install set"
    fi
    log "docker not found — installing via get.docker.com"
    curl -fsSL https://get.docker.com | sh
fi

if ! docker compose version >/dev/null 2>&1; then
    die "docker compose v2 plugin is required (try: apt-get install docker-compose-plugin)"
fi

# ── Step 2 — device identity ─────────────────────────────────

log "writing device identity to ${IDENTITY_DIR}/device.json"
mkdir -p "$IDENTITY_DIR"
chmod 755 "$IDENTITY_DIR"

# Reject silent identity rotation. Operators who actually want
# to repurpose a box and re-claim it elsewhere must pass --force.
if [[ -f "${IDENTITY_DIR}/device.json" && "$FORCE" -ne 1 ]]; then
    existing=$(grep -o '"device_id"[^,}]*' "${IDENTITY_DIR}/device.json" | head -1 || true)
    if [[ -n "$existing" && "$existing" != *"$DEVICE_ID"* ]]; then
        die "device.json already exists with a different device_id; use --force to overwrite"
    fi
fi

cat > "${IDENTITY_DIR}/device.json" <<JSON
{
  "device_id": "${DEVICE_ID}",
  "device_secret_b64": "${DEVICE_SECRET}",
  "sku": "${SKU}",
  "hardware_revision": "${HARDWARE_LABEL}"
}
JSON
chmod 600 "${IDENTITY_DIR}/device.json"

# ── Step 3 — compose + env ───────────────────────────────────

log "installing compose stack at ${INSTALL_ROOT}"
mkdir -p "$INSTALL_ROOT"
chmod 755 "$INSTALL_ROOT"

curl -fsSL "$COMPOSE_URL" -o "${INSTALL_ROOT}/docker-compose.yml"

# Keep the existing .env if one exists so operators who've
# tuned OUTBOX_PATH / EDGE_PUBLIC_HOST / etc. don't lose
# their work. We strip OXY_URL (and TS_AUTHKEY when a fresh one
# is being written, or when --clear-ts-authkey is set) so
# re-running with new values doesn't leave a stale duplicate
# line below the fresh one — bash dotenv-style files honor the
# last definition, but operators read top-down and get confused.
{
    if [[ -f "${INSTALL_ROOT}/.env" ]]; then
        if [[ -n "$TS_AUTHKEY" || "$CLEAR_TS_AUTHKEY" -eq 1 ]]; then
            grep -vE '^(OXY_URL|TS_AUTHKEY)=' "${INSTALL_ROOT}/.env" || true
        else
            grep -v '^OXY_URL=' "${INSTALL_ROOT}/.env" || true
        fi
    fi
    printf 'OXY_URL=%s\n' "$OXY_URL"
    if [[ -n "$TS_AUTHKEY" ]]; then
        printf 'TS_AUTHKEY=%s\n' "$TS_AUTHKEY"
    fi
} > "${INSTALL_ROOT}/.env.new"
mv "${INSTALL_ROOT}/.env.new" "${INSTALL_ROOT}/.env"
chmod 600 "${INSTALL_ROOT}/.env"

if [[ "$CLEAR_TS_AUTHKEY" -eq 1 && -z "$TS_AUTHKEY" ]]; then
    log "--clear-ts-authkey set without a new --ts-authkey; TS_AUTHKEY removed from .env"
fi

# ── Step 4 — systemd unit ────────────────────────────────────

log "installing systemd unit at ${SYSTEMD_PATH}"
curl -fsSL "$SYSTEMD_URL" -o "$SYSTEMD_PATH"
systemctl daemon-reload
systemctl enable "$SERVICE_NAME"
systemctl restart "$SERVICE_NAME"

# ── Step 5 — remote-support helpers (always installed, not run) ──
#
# Drop enable/disable scripts so the customer can opt in/out later
# even if they didn't pass --allow-remote-support at install. The
# scripts themselves are inert until the customer runs the enable
# one with a host auth key.
log "installing remote-support helpers under ${INSTALL_ROOT}"
curl -fsSL "$ENABLE_SUPPORT_URL"  -o "${INSTALL_ROOT}/enable-remote-support.sh"
curl -fsSL "$DISABLE_SUPPORT_URL" -o "${INSTALL_ROOT}/disable-remote-support.sh"
chmod 755 "${INSTALL_ROOT}/enable-remote-support.sh" "${INSTALL_ROOT}/disable-remote-support.sh"

# ── Step 6 — optionally enable remote support now ────────────
if [[ "$ALLOW_REMOTE_SUPPORT" -eq 1 ]]; then
    log "running enable-remote-support.sh (--allow-remote-support set)"
    "${INSTALL_ROOT}/enable-remote-support.sh" --authkey "$TS_HOST_AUTHKEY"
fi

# ── Step 7 — sanity tail ─────────────────────────────────────

log "service started; tailing journal for 10s"
timeout 10s journalctl -u "$SERVICE_NAME" -f --no-pager || true

cat <<DONE

✓ oxy-edge installed on $(hostname).

What just happened:
  • Identity:       ${IDENTITY_DIR}/device.json (0600)
  • Compose stack:  ${INSTALL_ROOT}/
  • Service:        ${SERVICE_NAME}
  • Remote support: $(if [[ "$ALLOW_REMOTE_SUPPORT" -eq 1 ]]; then echo "ENABLED (Oxy operators tagged tag:oxy-support can SSH)"; else echo "off — run ${INSTALL_ROOT}/enable-remote-support.sh to opt in"; fi)

Useful commands:
  systemctl status ${SERVICE_NAME}
  journalctl -u ${SERVICE_NAME} -f
  cd ${INSTALL_ROOT} && docker compose ps
  cd ${INSTALL_ROOT} && docker compose logs -f worker

The worker will announce + bootstrap against the pending claim
within ~5 minutes. Check the boxes table on ${OXY_URL} to confirm
the new device transitions from "waiting to bootstrap" to "active".
DONE
