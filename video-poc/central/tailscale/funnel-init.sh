#!/bin/sh
# Tailscale serve + funnel bootstrap.
#
# Runs once on compose-up as a sidecar of the `tailscale` service.
# Three jobs:
#
#   1. Wait for the tailscale daemon to finish logging in (an
#      authed daemon's `tailscale status --json` exposes a
#      `"DNSName"` for `Self`; the unauthed shape doesn't).
#   2. Apply `tailscale serve` so the public hostname's `/`
#      path proxies to MediaMTX's WHEP port, and `tailscale
#      funnel` so the public internet can reach it. The
#      previous static `funnel.json` approach failed because
#      tailscale doesn't expand `${TS_CERT_DOMAIN}` placeholders
#      in that file.
#   3. Read the resolved hostname back out of `tailscale status
#      --json` and write `EDGE_FUNNEL_HOSTNAME=...` into the
#      shared outbox so the worker's entrypoint can source it
#      on next boot — no operator hand-config needed.
#
# Exit shape (matters because compose treats non-zero as
# restart-eligible):
#   * Daemon-login timeout (TS_AUTHKEY missing or wrong tag) →
#     exit 0. Live preview stays disabled; rest of the pipeline
#     keeps working; no restart churn.
#   * `tailscale funnel` apply failure (MTX or TURN target) →
#     exit 1 so compose's `restart: on-failure` re-runs the
#     sidecar. The funnel apply is idempotent — a successful
#     prior apply is a no-op on retry.
#
# Future hardening (deferred): wrap the funnel applies in a
# bounded backoff loop so transient errors don't cycle the whole
# sidecar via compose restart.

set -eu

WAIT_ATTEMPTS=300   # 300 × 2s = 10min total
WAIT_INTERVAL_S=2
OUTBOX_DIR="/var/lib/outbox"
FUNNEL_ENV_PATH="${OUTBOX_DIR}/funnel.env"
MTX_WHEP_URL="${MTX_WHEP_URL:-http://mediamtx:8889}"
# Coturn TURN endpoint. Tailscale Funnel only allows public ports
# 443 / 8443 / 10000, so we land TURN on 8443. The browser uses
# `turns:<funnel>:8443?transport=tcp` (advertised in the WHEP
# session's ice_servers); Tailscale TLS-terminates and forwards
# plain TCP TURN traffic to coturn:8443.
COTURN_TCP_TARGET="${COTURN_TCP_TARGET:-tcp://video-poc-coturn:8443}"

log() { printf '[funnel-init] %s\n' "$*"; }

log "waiting for tailscale daemon to log in (up to $((WAIT_ATTEMPTS * WAIT_INTERVAL_S))s)"

# Helper: extract a string field from `tailscale status --json`.
# Pretty-printed JSON has newlines + spaces between fields, so we
# flatten with `tr -d '\n'` and use a whitespace-tolerant pattern.
# Avoids pulling in python or jq, which the Alpine-based
# tailscale/tailscale:stable image doesn't ship.
#
# Tracked brittleness: if the TS JSON layout changes (new nested
# fields under Self, e.g.) the Self-anchored extractor below
# could mis-anchor. Cheapest hardening: `apk add --no-cache jq`
# at the top of this script and use `jq -r '.Self.DNSName'`.
# Skipped today to keep the sidecar image untouched.
ts_field() {
    tailscale status --json 2>/dev/null \
        | tr -d '\n' \
        | sed -nE "s/.*\"$1\"[[:space:]]*:[[:space:]]*\"([^\"]+)\".*/\1/p"
}

# Self-anchored extraction. `DNSName` appears once in `Self` and
# once per peer; greedy `.*` would land on the last peer instead
# of Self. `[^{}]*` after `"Self":{` keeps us inside the Self
# object (Self has nested *arrays* like TailscaleIPs but no
# nested objects in the field range we care about).
ts_self_field() {
    tailscale status --json 2>/dev/null \
        | tr -d '\n' \
        | sed -nE "s/.*\"Self\"[[:space:]]*:[[:space:]]*\\{[^{}]*\"$1\"[[:space:]]*:[[:space:]]*\"([^\"]+)\".*/\1/p"
}

attempt=0
while [ "$attempt" -lt "$WAIT_ATTEMPTS" ]; do
    # `BackendState` reaches "Running" only after the daemon
    # has fully authed + come up; before that the JSON either
    # has `NeedsLogin` or no status at all.
    state=$(ts_field BackendState)
    if [ "$state" = "Running" ]; then
        log "tailscale BackendState=Running"
        break
    fi
    attempt=$((attempt + 1))
    sleep "$WAIT_INTERVAL_S"
done

if [ "${state:-}" != "Running" ]; then
    log "timed out waiting for tailscale login (state=${state:-unknown}); skipping funnel apply"
    log "set TS_AUTHKEY in /opt/oxy-edge/.env and restart oxy-edge.service to retry"
    # Exit 0 so compose doesn't mark the sidecar failed and
    # spam restarts — the rest of the stack is healthy.
    exit 0
fi

# Pull the public hostname. `Self.DNSName` looks like
# `video-poc-edge.tail123abc.ts.net.` (trailing dot). Must use the
# Self-anchored helper because every peer's entry also has a
# `"DNSName"` field.
dns_name=$(ts_self_field DNSName)
hostname=${dns_name%.}

if [ -z "$hostname" ]; then
    log "tailscale is up but Self.DNSName is empty; can't apply funnel"
    exit 0
fi

# `tailscale funnel --bg --https=443 <target>` is the combined
# form: enables Funnel on 443 AND sets the proxy target in one
# call. The older two-step pattern (`tailscale serve … && tailscale
# funnel <port>`) is fragile — the second command tries to enable
# Funnel on a port whose serve config it doesn't know, and ends up
# overwriting the serve with a default `http://127.0.0.1:<port>`
# proxy target. That's exactly the "WHEP ECONNTIMEOUT in the
# browser" failure mode — the public URL responds but proxies to
# nothing on port 443, not MTX on 8889.
log "applying tailscale funnel --bg --https=443 ${MTX_WHEP_URL}"
tailscale funnel --bg --https=443 "$MTX_WHEP_URL" || {
    log "funnel apply failed (mtx/whep); will retry on next sidecar boot"
    exit 1
}

# Tailscale TLS-terminates this and forwards plain TCP to coturn.
# `--tls-terminated-tcp` means "accept TLS on public 8443, hand the
# inner stream to <target> as plain TCP." Coturn listens for
# plain TURN on its alt port 8443 (see docker-compose.yml). The
# browser dials `turns:<funnel>:8443?transport=tcp` which speaks
# TURN inside the TLS tunnel; once Tailscale strips the TLS, coturn
# sees the TURN packets directly.
log "applying tailscale funnel --bg --tls-terminated-tcp=8443 ${COTURN_TCP_TARGET}"
tailscale funnel --bg --tls-terminated-tcp=8443 "$COTURN_TCP_TARGET" || {
    log "funnel apply failed (turn/8443); will retry on next sidecar boot"
    exit 1
}

mkdir -p "$OUTBOX_DIR"
# Atomic write: tmpfile + rename. The worker's entrypoint may
# race-read this file; a partial write would leave the env var
# missing and trigger the "funnel_hostname not reported" 503
# the moment the operator opens a live preview.
tmp="${FUNNEL_ENV_PATH}.tmp"
printf 'EDGE_FUNNEL_HOSTNAME=%s\n' "$hostname" > "$tmp"
mv "$tmp" "$FUNNEL_ENV_PATH"
log "wrote EDGE_FUNNEL_HOSTNAME=${hostname} to ${FUNNEL_ENV_PATH}"

# One-glance verification for the operator tailing logs.
tailscale serve status || true
tailscale funnel status || true
