#!/usr/bin/env bash
# Edge container entrypoint, shared by the `edge` worker and the
# `update-agent` services (both built from this image).
#
# Provisioning split-of-responsibility:
#   - The `edge` worker is the ONLY container that calls
#     `worker.provision`. Provision performs the
#     announce → bootstrap dance against the control plane,
#     materializes `edge_boxes` server-side, and writes
#     `$EDGE_ENV_PATH` with the credentials.
#   - `update-agent` (and any future sidecar that runs against the
#     same identity) WAITS for that env file to appear on the
#     shared outbox volume, then sources it and exec's its CMD.
#     Calling `worker.provision` from a second container is what
#     used to race the bootstrap path: both calls would land within
#     the same millisecond, both would SELECT the same
#     `pending_claim`, and `register_edge_box` would mint two
#     edge_box rows for one device — one became the live box, the
#     other was orphaned forever. See `service/fleet.rs` for the
#     server-side belt-and-suspenders fix; this entrypoint is the
#     first line of defence.
#
# Role selection: which path to take is controlled by
# `OXY_PROVISION_ROLE` (set in docker-compose.yml per-service). The
# default `"primary"` keeps the historical behaviour — useful for
# anyone running the image standalone (no separate update-agent),
# where one container has to do the bootstrap. `"sidecar"` is the
# wait-then-source path.
#
# Idempotency (primary path):
#   - `worker.provision` short-circuits with exit 0 when
#     `$EDGE_ENV_PATH` already exists, so re-starts of an already-
#     bootstrapped container cost one Python startup + a stat (~50 ms).
#   - When the file's missing, provision announces + bootstraps,
#     writes the env, and exits 0. We then source and exec.
#
# Recovery:
#   - Provision failures (no identity / unreachable Oxy / bootstrap
#     500) propagate as non-zero exit codes. compose's
#     restart: unless-stopped restarts the container; provision
#     retries on the next attempt. Transient network errors inside
#     the announce loop don't crash the container — provision
#     handles those internally with backoff.
#   - Sidecars wait indefinitely (with a poll log every 30s) until
#     `$EDGE_ENV_PATH` appears; if the primary never bootstraps,
#     the sidecar simply sits idle rather than racing in.

set -euo pipefail

EDGE_ENV_PATH="${OXY_EDGE_ENV_PATH:-/var/lib/outbox/edge.env}"
# Written by the `tailscale-funnel-init` sidecar after it
# applies serve + funnel against the live tailscale daemon.
# Sourcing it here lets the worker pick up
# `EDGE_FUNNEL_HOSTNAME` without the operator hand-pasting it
# into `/opt/oxy-edge/.env`. A literal env var (from compose's
# `.env`) still wins as an override — see the source order below.
FUNNEL_ENV_PATH="${OXY_FUNNEL_ENV_PATH:-/var/lib/outbox/funnel.env}"
ROLE="${OXY_PROVISION_ROLE:-primary}"

case "$ROLE" in
    primary)
        # Run provision. Exits 0 if already onboarded, 0 after a
        # successful bootstrap, non-zero on fatal config errors.
        python -m worker.provision
        ;;
    sidecar)
        # Wait for the primary to write the env. 2s poll cadence is
        # cheap and gives a fresh boot a sub-2s handoff in the
        # common case; a 30s heartbeat log keeps operators informed
        # if the primary is stuck.
        waited=0
        while [[ ! -f "$EDGE_ENV_PATH" ]]; do
            if (( waited % 30 == 0 )); then
                echo "{\"ts\":\"$(date -u +%FT%T)\",\"level\":\"info\",\"msg\":\"sidecar.waiting_for_primary\",\"edge_env_path\":\"${EDGE_ENV_PATH}\",\"waited_s\":${waited}}"
            fi
            sleep 2
            waited=$((waited + 2))
        done
        echo "{\"ts\":\"$(date -u +%FT%T)\",\"level\":\"info\",\"msg\":\"sidecar.primary_ready\",\"edge_env_path\":\"${EDGE_ENV_PATH}\",\"waited_s\":${waited}}"
        ;;
    *)
        echo "entrypoint: unknown OXY_PROVISION_ROLE=${ROLE} (expected 'primary' or 'sidecar')" >&2
        exit 2
        ;;
esac

# Funnel-hostname resolution model:
#
#   - **Default**: auto-discovery. `tailscale-funnel-init` reads
#     `Self.DNSName` after the daemon logs in, writes
#     `EDGE_FUNNEL_HOSTNAME=...` to `$FUNNEL_ENV_PATH`, and the
#     `source` below picks it up. Operator does nothing.
#   - **Override**: deployments running tailscale outside this
#     compose stack (separate host daemon, custom cert domain)
#     set `EDGE_FUNNEL_HOSTNAME_OVERRIDE=...` in compose's `.env`.
#     We snapshot that value before sourcing funnel.env, then
#     re-export it as `EDGE_FUNNEL_HOSTNAME` afterward so it wins.
#
# Why an `_OVERRIDE` suffix instead of just reading
# `EDGE_FUNNEL_HOSTNAME` directly: a stale `EDGE_FUNNEL_HOSTNAME=`
# line that an operator hand-pasted before auto-discovery existed
# used to silently shadow the discovered value. After a tailnet
# rename, that left the box pointing at a non-existent hostname
# and live preview broke with `ERR_CONNECTION_TIMED_OUT`. The
# rename forces the override to be deliberate.
ENV_FUNNEL_HOSTNAME_OVERRIDE="${EDGE_FUNNEL_HOSTNAME_OVERRIDE:-}"

# Source the bootstrap env regardless of role — primary just
# wrote it, sidecar just observed it. `set -a` exports every
# variable so the child CMD sees them.
if [[ -f "$EDGE_ENV_PATH" ]]; then
    set -a
    # shellcheck disable=SC1090
    source "$EDGE_ENV_PATH"
    set +a
fi

# Source the tailscale-funnel-init's hostname write. May not
# exist on a clean boot if the sidecar hasn't written it yet;
# we poll briefly (configurable; default 30s) so the common
# race — worker started before funnel-init finished its login +
# serve dance — doesn't require a manual `docker compose restart
# edge` to recover. With TS_AUTHKEY unset the file never
# appears; the wait expires, the worker comes up with
# `funnel_hostname_unset` and live preview is just disabled.
FUNNEL_ENV_WAIT_S="${OXY_FUNNEL_ENV_WAIT_S:-30}"
waited=0
while [[ ! -f "$FUNNEL_ENV_PATH" && "$waited" -lt "$FUNNEL_ENV_WAIT_S" ]]; do
    if (( waited == 0 )); then
        echo "{\"ts\":\"$(date -u +%FT%T)\",\"level\":\"info\",\"msg\":\"entrypoint.waiting_for_funnel_env\",\"path\":\"${FUNNEL_ENV_PATH}\",\"max_wait_s\":${FUNNEL_ENV_WAIT_S}}"
    fi
    sleep 1
    waited=$((waited + 1))
done
if [[ -f "$FUNNEL_ENV_PATH" ]]; then
    if (( waited > 0 )); then
        echo "{\"ts\":\"$(date -u +%FT%T)\",\"level\":\"info\",\"msg\":\"entrypoint.funnel_env_ready\",\"waited_s\":${waited}}"
    fi
    set -a
    # shellcheck disable=SC1090
    source "$FUNNEL_ENV_PATH"
    set +a
elif (( waited > 0 )); then
    echo "{\"ts\":\"$(date -u +%FT%T)\",\"level\":\"warn\",\"msg\":\"entrypoint.funnel_env_wait_timed_out\",\"waited_s\":${waited}}"
fi

# Restore the operator's explicit override if any. Has to
# happen AFTER both sources because the order above would
# otherwise let the funnel-init's value silently shadow a
# .env-set one.
if [[ -n "$ENV_FUNNEL_HOSTNAME_OVERRIDE" ]]; then
    export EDGE_FUNNEL_HOSTNAME="$ENV_FUNNEL_HOSTNAME_OVERRIDE"
fi

# Hand off to whatever CMD docker passed us.
exec "$@"
