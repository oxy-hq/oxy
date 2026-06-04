#!/usr/bin/env bash
# Revoke the remote-support path enabled by enable-remote-support.sh.
# Logs the host out of the tailnet, stops tailscaled, removes the
# state file. Container-side tailscaled (used for WebRTC Funnel)
# is left alone — different daemon, different scope.

set -euo pipefail

STATE_DIR="/var/lib/oxy"
STATE_FILE="${STATE_DIR}/remote-support"

log() { printf '\033[1;34m[remote-support]\033[0m %s\n' "$*"; }

[[ "$(id -u)" -eq 0 ]] || { echo "must run as root" >&2; exit 1; }

if command -v tailscale >/dev/null 2>&1; then
    log "logging host tailscaled out of the tailnet"
    tailscale logout || true
fi

if systemctl is-active tailscaled >/dev/null 2>&1; then
    log "stopping host tailscaled"
    systemctl stop tailscaled || true
    systemctl disable tailscaled || true
fi

rm -f "$STATE_FILE"

cat <<DONE

✓ Remote support disabled.

The host is no longer on the tailnet. Oxy operators cannot SSH this
box until you re-enable with enable-remote-support.sh.
DONE
