#!/usr/bin/env bash
# Enable Oxy operator SSH-via-tailnet on this edge box. Default-off
# (an out-of-the-box install has no SSH path at all); customer runs
# this to opt in.
#
# What it does:
#   1. Installs host `tailscaled` via apt if missing (the container's
#      tailscaled is userspace and isolated; it can't expose host
#      services).
#   2. Joins the tailnet as `tag:edge-box-host` (distinct from the
#      container's `tag:edge-box` so the SSH grant doesn't widen to
#      MTX / coturn).
#   3. Turns on Tailscale's built-in SSH server (`--ssh`). Listens on
#      the tailnet IP only; tailnet ACL gates *who* can connect.
#   4. Writes /var/lib/oxy/remote-support so `install-edge.sh` and
#      humans can see the opt-in state.
#
# Idempotent: re-running with the same args is a no-op. Re-running
# *without* --authkey but with `tailscaled` already joined just
# re-checks the SSH server is on.
#
# Revoke with `disable-remote-support.sh`.

set -euo pipefail

TS_HOST_AUTHKEY=""
HOSTNAME_OVERRIDE=""
STATE_DIR="/var/lib/oxy"
STATE_FILE="${STATE_DIR}/remote-support"

log()  { printf '\033[1;34m[remote-support]\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33m[remote-support]\033[0m %s\n' "$*" >&2; }
die()  { printf '\033[1;31m[remote-support]\033[0m %s\n' "$*" >&2; exit 1; }

usage() {
    cat <<EOF
Usage: $0 --authkey <tskey-auth-…> [--hostname <name>]

Required (first run):
  --authkey <tskey-…>   Tailscale auth key permitted to carry
                        tag:edge-box-host (see internal-docs/tailscale-acl.json).
Optional:
  --hostname <name>     Override the tailnet hostname (default: oxy-edge-host-\$(hostname))
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --authkey)   TS_HOST_AUTHKEY="$2"; shift 2 ;;
        --hostname)  HOSTNAME_OVERRIDE="$2"; shift 2 ;;
        -h|--help)   usage; exit 0 ;;
        *) warn "unknown arg: $1"; usage; exit 2 ;;
    esac
done

[[ "$(id -u)" -eq 0 ]] || die "must run as root (sudo); script touches /etc and /var/lib"

# 1. Install host tailscale if missing.
if ! command -v tailscale >/dev/null 2>&1; then
    log "installing tailscale on host via apt"
    apt-get update -qq
    apt-get install -y tailscale
fi

systemctl enable --now tailscaled

# 2. Join (or re-up) with the host tag + Tailscale SSH.
HOSTNAME_VALUE="${HOSTNAME_OVERRIDE:-oxy-edge-host-$(hostname)}"
if tailscale status --json 2>/dev/null | grep -q '"BackendState":[[:space:]]*"Running"'; then
    log "host already on tailnet; re-applying SSH + tag"
    tailscale set --advertise-tags=tag:edge-box-host --ssh=true
else
    [[ -n "$TS_HOST_AUTHKEY" ]] || die "host is not on the tailnet yet; pass --authkey <tskey-…>"
    log "joining tailnet as ${HOSTNAME_VALUE} (tag:edge-box-host, SSH on)"
    tailscale up \
        --authkey="$TS_HOST_AUTHKEY" \
        --advertise-tags=tag:edge-box-host \
        --hostname="$HOSTNAME_VALUE" \
        --ssh
fi

# 3. Resolve tailnet IP for the state file + user-facing message.
TS_IP="$(tailscale ip -4 || true)"
[[ -n "$TS_IP" ]] || die "could not read tailnet IPv4 address after join"

# 4. State file. Whoever else reads /var/lib/oxy gets a clear
#    signal that this opt-in is live.
mkdir -p "$STATE_DIR"
chmod 755 "$STATE_DIR"
cat > "$STATE_FILE" <<JSON
{
  "enabled_at": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "tailnet_ip": "${TS_IP}",
  "tailnet_hostname": "${HOSTNAME_VALUE}"
}
JSON
chmod 644 "$STATE_FILE"

cat <<DONE

✓ Remote support enabled.

Tailnet IP:        ${TS_IP}
Tailnet hostname:  ${HOSTNAME_VALUE}

Oxy operators carrying tag:oxy-support can now SSH this box.
Anyone else on the tailnet is denied by ACL.

To revoke:  sudo /opt/oxy-edge/disable-remote-support.sh
DONE
