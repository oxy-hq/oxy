"""Device-side first-boot provisioning (field-install flow).

Run from `worker/entrypoint.sh` before `worker.main` starts. Idempotent:
re-running on an already-bootstrapped box short-circuits at the
`edge.env exists` check.

Lifecycle:

  1. Load `DeviceIdentity` from `/var/lib/oxy/device.json` (written by
     `install-edge.sh` from the values the "Add device" wizard hands
     the operator).
  2. If `edge.env` already exists, exit 0 — this box is bootstrapped
     and the worker boot script will pick up the persisted env.
  3. POST `/fleet/announce/{device_id}` every `OXY_PROVISION_POLL_SECS`
     seconds until the server returns `state == "pending_claim"` or
     `state == "claimed"`. The "Add device" wizard pre-creates the
     pending claim, so the very first announce typically lands on
     `pending_claim` without any human polling.
  4. POST `/fleet/bootstrap/{device_id}` → receive
     `{workspace_id, edge_box_id, bearer}`. Persist these to
     `OXY_EDGE_ENV_PATH` (default `/var/lib/outbox/edge.env`) at 0600.

Network is the only side effect besides the env file. No docker /
systemd hooks here — the caller (the worker entrypoint) owns the
lifecycle. Provision exits non-zero on configuration errors; transient
network errors loop with backoff so a momentarily-unreachable Oxy
backend doesn't break a fresh install.

## What this module no longer does

The factory-enroll endpoint that used to gate device registration
behind a shared `OXY_FACTORY_ENROLL_SECRET` was removed when the
"Add device" wizard (`POST /{wid}/fleet/prepared-devices`) started
writing the device_registry row + pending_claim atomically
server-side. See `crates/cameras/src/routes/fleet.rs` for the
removal note. This module's older code path that did the factory
enroll, persisted a claim_code locally, and printed a URL for a
human to click is gone with it — the wizard is the single
human-action surface now.

Required env:
  OXY_URL              Base URL of the Oxy backend, including any
                       /api prefix (e.g. http://oxy:3000/api). Same
                       value the main worker reads.

Optional:
  OXY_EDGE_ENV_PATH    Where to write the bootstrap env file.
                       Default `/var/lib/outbox/edge.env` — writable
                       from inside the worker container (the outbox
                       volume), persists across container restarts.
  OXY_DEVICE_IDENTITY_PATH  Identity file location.
                       Default `/var/lib/oxy/device.json`.
  OXY_PROVISION_POLL_SECS  Announce poll interval. Default 30s.
"""
from __future__ import annotations

import argparse
import asyncio
import json
import os
from dataclasses import dataclass
from pathlib import Path
from uuid import UUID

import httpx

from .identity import DEFAULT_IDENTITY_PATH, DeviceIdentity
from .log import log

# The default lives on the outbox volume because the identity dir
# (/var/lib/oxy) is mounted read-only — install-edge.sh writes
# device.json there as root and we don't want the worker container
# overwriting that file. The outbox volume is already a writable
# named volume; reusing it keeps the compose surface small.
DEFAULT_EDGE_ENV_PATH = Path(os.environ.get("OXY_EDGE_ENV_PATH", "/var/lib/outbox/edge.env"))
DEFAULT_POLL_SECS = int(os.environ.get("OXY_PROVISION_POLL_SECS", "30"))


@dataclass
class ProvisionConfig:
    oxy_url: str
    identity_path: Path
    edge_env_path: Path
    poll_secs: int


def _config_from_env() -> ProvisionConfig:
    """Pull config from env. Single required var (`OXY_URL`) — every
    other knob has a sane default. Fails loudly on a missing
    `OXY_URL` rather than running with an empty value; the cost of
    silently announcing against the wrong URL is much higher than a
    clear startup error."""
    oxy_url = os.environ.get("OXY_URL", "").strip()
    if not oxy_url:
        raise SystemExit(
            "provision: missing required env var OXY_URL — set it in the "
            "edge service's environment block (it's the same value the "
            "main worker uses; see worker/provision.py docstring)."
        )
    return ProvisionConfig(
        oxy_url=oxy_url.rstrip("/"),
        identity_path=DEFAULT_IDENTITY_PATH,
        edge_env_path=DEFAULT_EDGE_ENV_PATH,
        poll_secs=DEFAULT_POLL_SECS,
    )


async def _announce_once(
    client: httpx.AsyncClient, cfg: ProvisionConfig, ident: DeviceIdentity
) -> dict:
    """One POST to /fleet/announce/{device_id}. Returns the parsed
    `{state, ...}` body; raises on non-2xx so the caller can decide
    whether to retry or treat as fatal."""
    ts, sig_b64 = ident.announce_signature_b64()
    r = await client.post(
        f"{cfg.oxy_url}/fleet/announce/{ident.device_id}",
        json={"timestamp": ts, "signature_b64": sig_b64},
    )
    r.raise_for_status()
    return r.json()


async def _bootstrap(
    client: httpx.AsyncClient, cfg: ProvisionConfig, ident: DeviceIdentity
) -> dict:
    """One POST to /fleet/bootstrap/{device_id}. Returns the parsed
    `{workspace_id, edge_box_id, bearer}` body. The bearer is the
    one-shot value the device persists + uses against `/control/*`
    from here on out."""
    ts, sig_b64 = ident.announce_signature_b64()
    r = await client.post(
        f"{cfg.oxy_url}/fleet/bootstrap/{ident.device_id}",
        json={"timestamp": ts, "signature_b64": sig_b64},
    )
    r.raise_for_status()
    return r.json()


def _write_edge_env(path: Path, payload: dict, device_id: UUID) -> None:
    """Write `EDGE_BOX_ID=…` / `EDGE_BOX_TOKEN=…` / `WORKSPACE_ID=…` /
    `BOOTSTRAP_DEVICE_ID=…` to a `.env` file the worker entrypoint
    sources. 0600 because the bearer is as sensitive as the original
    device secret in terms of "what can talk to /control/* as this box."

    `BOOTSTRAP_DEVICE_ID` records which `device.json` identity owned
    this bootstrap. On re-runs, [`_edge_env_device_id`] reads it back so
    we can detect when the operator re-installed with a fresh
    device-id and re-run bootstrap instead of silently sourcing a
    stale env (the failure mode that produced the `pending_claim`
    deadlock — pre-fix, the worker would JWT-sign with the new
    device-id while keeping the old EDGE_BOX_ID and 401 every poll).

    Atomic rename via .tmp so a crash mid-write doesn't leave a
    truncated file that future boots would silently source."""
    path.parent.mkdir(parents=True, exist_ok=True)
    body = (
        f"EDGE_BOX_ID={payload['edge_box_id']}\n"
        f"EDGE_BOX_TOKEN={payload['bearer']}\n"
        f"WORKSPACE_ID={payload['workspace_id']}\n"
        f"BOOTSTRAP_DEVICE_ID={device_id}\n"
    )
    tmp = path.with_suffix(".tmp")
    fd = os.open(tmp, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
    with os.fdopen(fd, "w") as f:
        f.write(body)
    os.replace(tmp, path)


def _edge_env_device_id(path: Path) -> UUID | None:
    """Return the `BOOTSTRAP_DEVICE_ID` recorded in an existing edge.env,
    or `None` if the file is missing the line (pre-fix vintage written
    before we started recording it). Treats parse errors as "missing"
    rather than raising — we'd rather short-circuit conservatively
    than crash the entrypoint on a malformed line."""
    try:
        for line in path.read_text().splitlines():
            if line.startswith("BOOTSTRAP_DEVICE_ID="):
                raw = line.split("=", 1)[1].strip()
                return UUID(raw) if raw else None
    except (OSError, ValueError):
        return None
    return None


async def run(cfg: ProvisionConfig) -> int:
    """Returns process exit code:
      0 — already onboarded OR claim+bootstrap succeeded
      2 — identity missing (install-edge.sh didn't run, or device.json
          got wiped); operator must re-run the install snippet
      3 — bootstrap call returned an error after the claim landed

    Transient network errors retry inside the announce loop rather
    than bubbling up — a brief WAN outage at first-boot shouldn't
    require human intervention.
    """
    ident = DeviceIdentity.load(cfg.identity_path)
    if ident is None:
        log(
            "error",
            "provision.identity_missing",
            identity_path=str(cfg.identity_path),
            hint=(
                "device.json was not written. Re-run the 'Add device' "
                "wizard install snippet on this box, OR drop a valid "
                "device.json at the path above."
            ),
        )
        return 2

    if cfg.edge_env_path.exists():
        recorded_device_id = _edge_env_device_id(cfg.edge_env_path)
        if recorded_device_id == ident.device_id:
            log(
                "info",
                "provision.already_onboarded",
                edge_env_path=str(cfg.edge_env_path),
                device_id=str(ident.device_id),
            )
            return 0
        if recorded_device_id is None:
            # Pre-fix edge.env (or hand-written one) — no
            # BOOTSTRAP_DEVICE_ID line. Trust it: rewriting bootstrap
            # against a stale claim wouldn't help anyway, and existing
            # working boxes shouldn't be disturbed on first upgrade.
            log(
                "info",
                "provision.already_onboarded",
                edge_env_path=str(cfg.edge_env_path),
                device_id=str(ident.device_id),
                note="edge.env predates BOOTSTRAP_DEVICE_ID; can't verify",
            )
            return 0
        # Identity drifted under us — the operator re-ran the wizard
        # and re-installed with a fresh device.json but the outbox
        # volume still carries the old bootstrap. Re-run announce +
        # bootstrap so the new claim row transitions to `claimed` and
        # the worker's JWT issuer matches the EDGE_BOX_ID it polls
        # against.
        log(
            "warn",
            "provision.identity_drift",
            edge_env_path=str(cfg.edge_env_path),
            previous_device_id=str(recorded_device_id),
            current_device_id=str(ident.device_id),
            action="re-running announce + bootstrap against current device.json",
        )

    log(
        "info",
        "provision.start",
        device_id=str(ident.device_id),
        oxy_url=cfg.oxy_url,
    )

    async with httpx.AsyncClient(timeout=20.0) as client:
        # Announce loop. The wizard pre-creates the pending claim,
        # so this typically settles on the first call. We still loop
        # because (a) the wizard's claim creation and this announce
        # race in time and (b) operators occasionally re-run the
        # install snippet before the wizard finishes — the announce
        # retries cover both.
        while True:
            try:
                resp = await _announce_once(client, cfg, ident)
            except httpx.HTTPError as exc:
                log("warn", "provision.announce_failed", error=str(exc))
                await asyncio.sleep(cfg.poll_secs)
                continue
            state = resp.get("state")
            if state in ("pending_claim", "claimed"):
                log("info", "provision.claim_ready", state=state)
                break
            # state == "unclaimed" or anything else → still waiting
            # on the wizard. Retry. log("info") because this is the
            # normal greenfield path while the operator is mid-click.
            log("info", "provision.waiting_for_claim", state=state)
            await asyncio.sleep(cfg.poll_secs)

        try:
            payload = await _bootstrap(client, cfg, ident)
        except httpx.HTTPError as exc:
            log("error", "provision.bootstrap_failed", error=str(exc))
            return 3

    _write_edge_env(cfg.edge_env_path, payload, ident.device_id)
    log(
        "info",
        "provision.bootstrap_complete",
        edge_box_id=payload["edge_box_id"],
        workspace_id=payload["workspace_id"],
        edge_env_path=str(cfg.edge_env_path),
    )
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--oxy-url",
        default=None,
        help="Override OXY_URL.",
    )
    parser.add_argument(
        "--once",
        action="store_true",
        help="Run a single announce + exit (debugging tool — no bootstrap).",
    )
    args = parser.parse_args()

    cfg = _config_from_env()
    if args.oxy_url:
        cfg = ProvisionConfig(**{**cfg.__dict__, "oxy_url": args.oxy_url.rstrip("/")})

    if args.once:
        # Smoke-test path: exercise identity + signing + announce
        # without committing to a bootstrap. Useful when triaging a
        # box that's stuck announcing — the bootstrap call costs the
        # backend a real edge_box row + bearer mint, so we don't
        # want it firing accidentally during diagnostics.
        ident = DeviceIdentity.load(cfg.identity_path)
        if ident is None:
            raise SystemExit(
                f"provision --once: no identity at {cfg.identity_path}"
            )

        async def _once() -> int:
            async with httpx.AsyncClient(timeout=10.0) as client:
                resp = await _announce_once(client, cfg, ident)
                print(json.dumps(resp, indent=2))
            return 0

        return asyncio.run(_once())

    return asyncio.run(run(cfg))


if __name__ == "__main__":
    raise SystemExit(main())
