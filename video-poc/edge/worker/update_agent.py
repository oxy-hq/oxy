"""Device-side software-update agent (IoT Phase 2).

Polls `GET /control/boxes/{id}/target` every ~5 minutes. When the
operator-set `target_image_tag` differs from what's running, the
agent pulls the new image and restarts the worker container via
docker compose. After the restart, the next health ping from the
worker stamps `current_image_tag = target` and the cycle quiets.

Designed to run as a small sidecar container in the same compose
stack as the worker, with the docker socket mounted:

    services:
      oxy-update-agent:
        image: ghcr.io/oxy/edge-update-agent:latest
        environment:
          OXY_URL: ${OXY_URL}
          EDGE_BOX_ID: ${EDGE_BOX_ID}
          EDGE_BOX_TOKEN: ${EDGE_BOX_TOKEN}
          # Worker compose service name we'll pull + restart.
          OXY_WORKER_SERVICE: worker
          # Compose file path inside the container; bind-mount the
          # host's docker-compose.yml at the same path.
          OXY_COMPOSE_FILE: /etc/oxy/docker-compose.yml
        volumes:
          - /var/run/docker.sock:/var/run/docker.sock:ro
          - /path/to/docker-compose.yml:/etc/oxy/docker-compose.yml:ro
        restart: unless-stopped

Why poll instead of long-poll / SSE: simpler, no idle connections,
the 5-minute cadence matches operator expectations ("how long until
my new tag rolls out?"). Trade-off: ~5min update latency, which is
the right ballpark for restaurant operators who set updates at
4am and expect them done by morning.

Watchdog rollback (#156): once we apply a new target tag, we
track whether the worker reports back on it via the next few
polls. After `_WATCHDOG_FAILURE_THRESHOLD` consecutive polls
where `current_image_tag` hasn't caught up to `target_image_tag`,
we treat the new image as broken: pull the previous tag (the
one the worker was running before we attempted the swap) and
re-apply, then POST `last_update_result="reverted"` so the
operator sees the rollback in the fleet dashboard.

held_until enforcement (#157): the server exposes a
`held_until` timestamp on the target response. When it's in the
future, we skip the apply and keep polling. Operators set this
during "don't touch this box during lunch rush" windows.

Skipped for V1 (deliberate):
* Beta/stable channels. V1 is per-box target only; operators set
  every box to the same tag for "stable" behavior.
"""
from __future__ import annotations

import asyncio
import json
import os
import subprocess
import sys
import tempfile
import time
from dataclasses import asdict, dataclass, field
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

import httpx

from .identity import DeviceIdentity
from .jwt_mint import BearerAuth, DeviceJwtAuth, JwtMinter
from .log import log

DEFAULT_POLL_SECS = int(os.environ.get("OXY_UPDATE_POLL_SECS", "300"))

# Watchdog tuning. After this many consecutive polls where we've
# applied a target but the worker still hasn't reported running
# it, we treat the new image as broken and roll back. Default 3
# at 5-minute cadence = ~15 minute window for a fresh container
# to come up — generous for a slow pull + boot + first health
# post, tight enough that a hung image doesn't sit there forever.
_WATCHDOG_FAILURE_THRESHOLD = int(
    os.environ.get("OXY_UPDATE_WATCHDOG_THRESHOLD", "3")
)

# Persistent watchdog state lives here. Mounted from the host so
# it survives container restart (the same mount the worker uses
# for device.json — see docker-compose.yml's `/var/lib/oxy` bind).
# An agent crash mid-rollout would otherwise lose pending_target +
# rollback_to and silently re-apply a known-bad image.
_DEFAULT_STATE_FILE = os.environ.get(
    "OXY_UPDATE_STATE_FILE", "/var/lib/oxy/update-state.json"
)

# Image GC cadence (#4 follow-up). Daily prune keeps a week of
# images for instant rollback while bounding eMMC consumption.
_GC_INTERVAL_SECS = int(os.environ.get("OXY_UPDATE_GC_INTERVAL_SECS", "86400"))
_GC_KEEP_AGE = os.environ.get("OXY_UPDATE_GC_KEEP_AGE", "168h")

# Adaptive watchdog (#5). The base window is 15min; we extend it to
# 3× the measured pull time when that's larger, so slow-network
# sites (cellular fallback) don't trip watchdog before the new
# image even finishes booting.
_WATCHDOG_BASE_SECS = int(os.environ.get("OXY_UPDATE_WATCHDOG_BASE_SECS", "900"))
_WATCHDOG_PULL_MULTIPLIER = float(
    os.environ.get("OXY_UPDATE_WATCHDOG_PULL_MULTIPLIER", "3.0")
)

# Cosign verification (CI #1). Defaults match the sigstore keyless
# signing the publish workflow uses. The identity regexp accepts
# any tag, branch, or workflow path on either of our publishing
# repos so the same agent code works with images cut from
# `oxy-hq/oxy` OR `oxy-hq/oxy-internal`.
#
# The escape hatch — OXY_DISABLE_SIGNATURE_VERIFY=true — is for
# the local POC compose stack where the image is built locally
# and not signed. Production deployments must never set this.
_COSIGN_DISABLED = os.environ.get("OXY_DISABLE_SIGNATURE_VERIFY", "").lower() == "true"
_COSIGN_IDENTITY_RE = os.environ.get(
    "OXY_COSIGN_IDENTITY_RE",
    r"^https://github\.com/oxy-hq/oxy(-internal)?/\.github/workflows/edge-image\.yml@.+$",
)
_COSIGN_OIDC_ISSUER = os.environ.get(
    "OXY_COSIGN_OIDC_ISSUER",
    "https://token.actions.githubusercontent.com",
)


@dataclass
class AgentConfig:
    oxy_url: str
    edge_box_id: str
    # `httpx.Auth` object the client uses on every `/control/*`
    # call. Built once at boot from either the device identity
    # (JWT) or the legacy bearer token; we never branch per-request
    # so JWT rotation stays transparent to the call sites — same
    # contract the worker uses.
    auth: httpx.Auth
    worker_service: str
    compose_file: str | None
    poll_secs: int


def _config_from_env() -> AgentConfig:
    """Read agent config from env. Fail loudly on a missing
    required variable — running with an empty URL would silently
    no-op every poll.

    Auth resolution mirrors the worker (see `worker.main`): when
    a device identity is on disk we mint JWTs from its HMAC
    secret; otherwise we fall back to the static EDGE_BOX_TOKEN
    bearer. After the 2026-06 cutover the backend only accepts
    JWT by default, so the bearer path is effectively a "tests
    + dev only" fallback (the missing-identity case shouldn't
    happen on a properly bootstrapped box)."""
    required = {
        "OXY_URL": os.environ.get("OXY_URL", "").strip(),
        "EDGE_BOX_ID": os.environ.get("EDGE_BOX_ID", "").strip(),
        "EDGE_BOX_TOKEN": os.environ.get("EDGE_BOX_TOKEN", "").strip(),
    }
    missing = [k for k, v in required.items() if not v]
    if missing:
        raise SystemExit(
            f"update-agent: missing required env vars: {', '.join(missing)}"
        )
    identity = DeviceIdentity.load()
    if identity is not None:
        auth: httpx.Auth = DeviceJwtAuth(
            JwtMinter(identity.device_id, identity.device_secret)
        )
        log("info", "update_agent.auth_mode", mode="jwt",
            device_id=str(identity.device_id))
    else:
        auth = BearerAuth(required["EDGE_BOX_TOKEN"])
        log("warn", "update_agent.auth_mode", mode="bearer",
            reason="no device identity on disk; bearer will 401 against post-cutover backends")
    return AgentConfig(
        oxy_url=required["OXY_URL"].rstrip("/"),
        edge_box_id=required["EDGE_BOX_ID"],
        auth=auth,
        worker_service=os.environ.get("OXY_WORKER_SERVICE", "worker"),
        compose_file=os.environ.get("OXY_COMPOSE_FILE") or None,
        poll_secs=DEFAULT_POLL_SECS,
    )


async def fetch_target(client: httpx.AsyncClient, cfg: AgentConfig) -> dict[str, Any] | None:
    """Single GET /control/boxes/{id}/target. Returns the parsed
    response, or None on transient errors (caller logs + retries).
    A 401/403 raises — the operator needs to fix auth (re-run the
    Add device wizard or restore the device identity file)."""
    try:
        # `cfg.auth` is the JWT minter (or legacy bearer) — see
        # `_config_from_env`. The httpx client injects
        # `Authorization` per-request and handles JWT rotation
        # transparently, so we never see the token here.
        r = await client.get(
            f"{cfg.oxy_url}/control/boxes/{cfg.edge_box_id}/target",
            auth=cfg.auth,
        )
    except httpx.HTTPError as exc:
        log("warn", "update_agent.target_fetch_failed", error=str(exc))
        return None
    if r.status_code in (401, 403):
        # Don't retry forever on auth — the credentials are wrong;
        # the operator must intervene.
        raise SystemExit(
            f"update-agent: auth rejected ({r.status_code}); check device.json + EDGE_BOX_TOKEN"
        )
    if r.status_code != 200:
        log("warn", "update_agent.target_fetch_non_200", status=r.status_code)
        return None
    try:
        return r.json()
    except ValueError:
        # 200 with non-JSON body — almost always an OXY_URL missing `/api`
        # so the request hit the SPA fallback. Surface enough to fix it
        # without source-diving (see `main.py::_fetch_config` for the
        # same pattern on the config endpoint).
        content_type = r.headers.get("content-type", "")
        body_snippet = r.text[:120].replace("\n", " ").strip()
        hint = (
            " (looks like HTML — OXY_URL likely needs `/api` appended)"
            if "html" in content_type.lower() or body_snippet.startswith("<")
            else ""
        )
        log(
            "warn",
            "update_agent.target_fetch_non_json",
            status=r.status_code,
            content_type=content_type,
            body=body_snippet,
            hint=hint,
        )
        return None


@dataclass
class _ApplyOutcome:
    """Result of one `apply_update` call. `pull_secs` lets the
    watchdog adapt to slow networks (see `_WatchdogState.watchdog_threshold`)."""

    ok: bool
    pull_secs: float = 0.0


def verify_signature(target_image_tag: str) -> bool:
    """Cosign-verify the target image against our publishing workflow
    identity. Returns True on success, False on any failure (which
    callers treat as a fail-closed signal — we will NOT pull an
    unverified image).

    Skipped entirely when `OXY_DISABLE_SIGNATURE_VERIFY=true`, which
    is appropriate for the local POC compose stack and for ANY
    environment where the image is built from a local Dockerfile
    rather than the signed CI release. Operators running production
    must leave this unset.
    """
    if _COSIGN_DISABLED:
        log("warn", "update_agent.cosign_disabled", target=target_image_tag)
        return True

    cmd = [
        "cosign",
        "verify",
        "--certificate-identity-regexp",
        _COSIGN_IDENTITY_RE,
        "--certificate-oidc-issuer",
        _COSIGN_OIDC_ISSUER,
        target_image_tag,
    ]
    log("info", "update_agent.cosign_verify_begin", target=target_image_tag)
    try:
        proc = subprocess.run(cmd, capture_output=True, text=True, timeout=60)
    except (subprocess.SubprocessError, FileNotFoundError) as exc:
        log(
            "error",
            "update_agent.cosign_exec_failed",
            target=target_image_tag,
            error=str(exc),
        )
        return False
    if proc.returncode != 0:
        log(
            "error",
            "update_agent.cosign_verify_failed",
            target=target_image_tag,
            returncode=proc.returncode,
            stderr=proc.stderr[-500:],
        )
        return False
    log("info", "update_agent.cosign_verify_ok", target=target_image_tag)
    return True


def apply_update(cfg: AgentConfig, target_image_tag: str) -> _ApplyOutcome:
    """Pull the new image and restart the worker container via
    docker compose. Returns the outcome including measured pull
    time so the caller can size the watchdog window adaptively.

    Why subprocess instead of docker SDK: the agent container is
    smaller without docker-py, and `docker compose` is the contract
    operators see in our reference deployment — keeping the same
    commands here makes manual recovery (operator runs the same
    commands by hand) symmetric with what the agent does.

    Stdout/stderr are captured + logged so a stuck pull doesn't
    swallow its own diagnostic. The 10-min timeout matches the
    upper bound on a slow-network pull of a ~500MB ML image."""
    # Fail-closed signature check — refuses to pull an image whose
    # provenance doesn't match our publishing workflow. The escape
    # hatch (OXY_DISABLE_SIGNATURE_VERIFY) is documented in
    # internal-docs/edge-image-releases.md.
    if not verify_signature(target_image_tag):
        return _ApplyOutcome(ok=False, pull_secs=0.0)

    pull_cmd = ["docker", "pull", target_image_tag]
    restart_cmd = (
        [
            "docker",
            "compose",
            "-f",
            cfg.compose_file,
            "up",
            "-d",
            "--force-recreate",
            cfg.worker_service,
        ]
        if cfg.compose_file is not None
        # Bare-docker fallback (no compose file): just restart the
        # worker container by name. Operators using this path must
        # name their worker container `oxy-worker`.
        else ["docker", "restart", "oxy-worker"]
    )

    pull_secs = 0.0
    for cmd in (pull_cmd, restart_cmd):
        log("info", "update_agent.exec", cmd=" ".join(cmd))
        start = time.monotonic()
        try:
            proc = subprocess.run(
                cmd,
                capture_output=True,
                text=True,
                timeout=10 * 60,
            )
        except (subprocess.SubprocessError, FileNotFoundError) as exc:
            log("error", "update_agent.exec_failed", cmd=" ".join(cmd), error=str(exc))
            return _ApplyOutcome(ok=False, pull_secs=pull_secs)
        elapsed = time.monotonic() - start
        if cmd is pull_cmd:
            pull_secs = elapsed
        if proc.returncode != 0:
            log(
                "error",
                "update_agent.exec_nonzero",
                cmd=" ".join(cmd),
                returncode=proc.returncode,
                stderr=proc.stderr[-500:],
            )
            return _ApplyOutcome(ok=False, pull_secs=pull_secs)
    return _ApplyOutcome(ok=True, pull_secs=pull_secs)


def gc_images() -> None:
    """Daily prune of old images. `--until=168h` keeps a week of
    images so instant rollback to last week's tag still works
    without re-pulling. Errors are logged and swallowed — failing
    GC must not block the apply loop."""
    cmd = ["docker", "image", "prune", "-af", f"--filter=until={_GC_KEEP_AGE}"]
    log("info", "update_agent.gc_begin", cmd=" ".join(cmd))
    try:
        proc = subprocess.run(cmd, capture_output=True, text=True, timeout=300)
    except (subprocess.SubprocessError, FileNotFoundError) as exc:
        log("warn", "update_agent.gc_failed", error=str(exc))
        return
    if proc.returncode != 0:
        log(
            "warn",
            "update_agent.gc_nonzero",
            returncode=proc.returncode,
            stderr=proc.stderr[-500:],
        )
        return
    # `docker image prune` prints "Total reclaimed space: X" on the
    # last line. Useful in audit; not parsed.
    log("info", "update_agent.gc_done", stdout=proc.stdout[-500:])


@dataclass
class _WatchdogState:
    """Persistent state for the rollback watchdog.

    Reset on every successful convergence (current catches up
    to target) AND on every successful revert. Persisted to
    disk via `save()` on every transition so an agent crash
    mid-rollout doesn't silently re-apply a known-bad target
    on restart.
    """

    # Tag we most recently triggered `apply_update` for. None
    # when no apply is currently in flight.
    pending_target: str | None = None
    # `current_image_tag` from the server at the moment we
    # triggered the apply — this is the rollback baseline. None
    # outside an active watchdog window OR when the apply was
    # a first-ever apply (no prior known-good tag exists).
    rollback_to: str | None = None
    # Consecutive polls where pending_target hasn't shown up as
    # the worker's current_image_tag. Hits the adaptive watchdog
    # threshold (see `watchdog_threshold` below) → revert.
    failure_count: int = 0
    # Last measured `docker pull` wall-clock duration in seconds.
    # Drives the adaptive watchdog window — slow pulls get a
    # proportionally longer convergence budget.
    last_pull_secs: float = 0.0
    # When was the last `docker image prune` run? Epoch seconds.
    # Drives the daily GC cadence.
    last_gc_at: float = 0.0
    # Where this struct came from on disk. Not serialized.
    _path: Path | None = field(default=None, compare=False, repr=False)

    @classmethod
    def load(cls, path: str) -> "_WatchdogState":
        """Read the persisted state, or return a fresh struct if
        the file is missing / unreadable / corrupt. We never raise
        — bad state on disk shouldn't prevent the agent from
        starting, it just means a degraded first cycle."""
        p = Path(path)
        try:
            raw = p.read_text()
        except (FileNotFoundError, PermissionError) as exc:
            log("info", "update_agent.state_load_missing", path=str(p), reason=str(exc))
            return cls(_path=p)
        try:
            data = json.loads(raw)
        except json.JSONDecodeError as exc:
            log("warn", "update_agent.state_load_corrupt", path=str(p), error=str(exc))
            return cls(_path=p)
        # Filter to known keys so an old schema with extra fields
        # doesn't blow up; missing keys fall back to defaults.
        allowed = {
            "pending_target",
            "rollback_to",
            "failure_count",
            "last_pull_secs",
            "last_gc_at",
        }
        clean = {k: v for k, v in data.items() if k in allowed}
        out = cls(**clean, _path=p)
        log(
            "info",
            "update_agent.state_loaded",
            path=str(p),
            pending_target=out.pending_target,
            rollback_to=out.rollback_to,
            failure_count=out.failure_count,
        )
        return out

    def save(self) -> None:
        """Atomic write via temp-file + rename. The pattern guarantees
        a reader either sees the old contents in full or the new
        contents in full — no partial JSON during a crash. Best-effort:
        a failed write logs and moves on; the in-memory state still
        drives the current process."""
        if self._path is None:
            return
        try:
            self._path.parent.mkdir(parents=True, exist_ok=True)
            payload = {
                "pending_target": self.pending_target,
                "rollback_to": self.rollback_to,
                "failure_count": self.failure_count,
                "last_pull_secs": self.last_pull_secs,
                "last_gc_at": self.last_gc_at,
            }
            # Use the same directory so the rename is atomic on
            # the same filesystem (cross-fs rename copies).
            fd, tmp = tempfile.mkstemp(
                prefix=".update-state-", suffix=".json", dir=str(self._path.parent)
            )
            try:
                with os.fdopen(fd, "w") as f:
                    json.dump(payload, f)
                os.replace(tmp, self._path)
            except Exception:
                # Clean up the temp file on any failure path.
                try:
                    os.unlink(tmp)
                except OSError:
                    pass
                raise
        except (OSError, PermissionError) as exc:
            log("warn", "update_agent.state_save_failed", error=str(exc))

    def watchdog_threshold(self, poll_secs: int) -> int:
        """Adaptive watchdog window. Base is `_WATCHDOG_FAILURE_THRESHOLD`
        polls (~15min at 5-min cadence). When the last pull was slow,
        extend proportionally so a cellular site doesn't trip
        watchdog before the new image even finishes booting."""
        base = _WATCHDOG_FAILURE_THRESHOLD
        if self.last_pull_secs <= 0:
            return base
        adaptive_secs = max(_WATCHDOG_BASE_SECS, self.last_pull_secs * _WATCHDOG_PULL_MULTIPLIER)
        adaptive = int(adaptive_secs // max(poll_secs, 1)) + 1
        return max(base, adaptive)


def _parse_held_until(raw: Any) -> datetime | None:
    """Server sends RFC-3339 UTC; coerce to a tz-aware datetime
    or None on parse failure. We never raise — a bad value just
    means we won't honor the hold this cycle."""
    if not raw or not isinstance(raw, str):
        return None
    try:
        # Tolerate trailing Z (RFC 3339) — datetime.fromisoformat
        # in <3.11 doesn't accept it; we use 3.11+ but coerce
        # defensively for forward-compat with older Pythons.
        s = raw.replace("Z", "+00:00") if raw.endswith("Z") else raw
        return datetime.fromisoformat(s)
    except (TypeError, ValueError):
        return None


async def _report_update_result(
    client: httpx.AsyncClient,
    cfg: AgentConfig,
    result: str,
) -> None:
    """Send a minimal box-health POST stamping `last_update_result`.
    All other fields stay None — the regular worker health loop
    will refresh them on its next tick. Best-effort: a failed
    report logs + returns, never raises."""
    try:
        r = await client.post(
            f"{cfg.oxy_url}/control/boxes/{cfg.edge_box_id}/health",
            auth=cfg.auth,
            json={
                "last_update_result": result,
            },
        )
        if r.status_code >= 400:
            log(
                "warn",
                "update_agent.result_report_failed",
                status=r.status_code,
                result=result,
            )
    except httpx.HTTPError as exc:
        log("warn", "update_agent.result_report_error", error=str(exc), result=result)


async def run(cfg: AgentConfig) -> int:
    """Main loop. Returns an exit code; 0 on clean shutdown
    (we don't return naturally — this loops until killed)."""
    state = _WatchdogState.load(_DEFAULT_STATE_FILE)
    log(
        "info",
        "update_agent.start",
        edge_box_id=cfg.edge_box_id,
        poll_secs=cfg.poll_secs,
        watchdog_threshold=_WATCHDOG_FAILURE_THRESHOLD,
        state_file=_DEFAULT_STATE_FILE,
    )
    async with httpx.AsyncClient(timeout=30.0) as client:
        while True:
            await _tick(client, cfg, state)
            _maybe_gc(state)
            await asyncio.sleep(cfg.poll_secs)


def _maybe_gc(state: _WatchdogState) -> None:
    """Run image GC at most once per `_GC_INTERVAL_SECS`. We piggy-
    back on the tick loop rather than spawn a separate timer so the
    agent has one source of truth for time."""
    now = time.time()
    if now - state.last_gc_at < _GC_INTERVAL_SECS:
        return
    gc_images()
    state.last_gc_at = now
    state.save()


async def _tick(
    client: httpx.AsyncClient,
    cfg: AgentConfig,
    state: _WatchdogState,
) -> None:
    """One iteration of the poll loop. Pulled out as a separate
    function so unit tests can drive it with a fake `httpx`
    client and a fake clock without spinning the real loop."""
    target = await fetch_target(client, cfg)
    if target is None:
        return

    target_tag: str | None = target.get("target_image_tag")
    current_tag: str | None = target.get("current_image_tag")
    held_until = _parse_held_until(target.get("held_until"))

    # Watchdog first — convergence check applies even when target
    # is None (operator could have cleared it mid-rollout, in
    # which case we want to release pending state).
    if state.pending_target is not None:
        if current_tag == state.pending_target:
            log(
                "info",
                "update_agent.converged",
                target=state.pending_target,
                polls=state.failure_count,
            )
            state.pending_target = None
            state.rollback_to = None
            state.failure_count = 0
            state.save()
        else:
            state.failure_count += 1
            threshold = state.watchdog_threshold(cfg.poll_secs)
            log(
                "warn",
                "update_agent.watchdog_no_convergence",
                target=state.pending_target,
                current=current_tag,
                consecutive_failures=state.failure_count,
                threshold=threshold,
            )
            if state.failure_count >= threshold:
                await _rollback(client, cfg, state)
            else:
                state.save()
            # Whether we rolled back or are still waiting for
            # convergence, this tick is DONE. Skipping the
            # apply path below ensures we don't re-trigger the
            # same broken apply while the previous restart is
            # still in flight — re-applying every tick would
            # both waste pulls and reset the failure_count we
            # just incremented.
            return

    if not target_tag:
        # Operator hasn't set a target — automated updates are
        # off for this box. Quiet path.
        log("debug", "update_agent.no_target_configured")
        return

    if target_tag == current_tag:
        log(
            "debug",
            "update_agent.already_at_target",
            target=target_tag,
        )
        return

    # held_until enforcement (#157). Skip the apply but keep
    # polling; the worker's regular health pings still flow so
    # operators can see the box is healthy during the hold.
    if held_until is not None and held_until > datetime.now(timezone.utc):
        log(
            "info",
            "update_agent.held",
            target=target_tag,
            held_until=held_until.isoformat(),
        )
        return

    log(
        "info",
        "update_agent.applying",
        from_=current_tag,
        to=target_tag,
    )
    outcome = apply_update(cfg, target_tag)
    log(
        "info" if outcome.ok else "error",
        "update_agent.apply_result",
        target=target_tag,
        applied=outcome.ok,
        pull_secs=round(outcome.pull_secs, 2),
    )
    # Record the pull time even on a failed restart — the docker
    # pull itself usually succeeded, and a long pull time should
    # still extend the next watchdog window.
    if outcome.pull_secs > 0:
        state.last_pull_secs = outcome.pull_secs
    if outcome.ok:
        # Arm the watchdog — current_tag is the rollback baseline
        # in case the new image doesn't come up. If current_tag is
        # None we have no known-good to roll back TO; honest semantics
        # in `_rollback` will surface that to the operator instead of
        # silently claiming success.
        state.pending_target = target_tag
        state.rollback_to = current_tag
        state.failure_count = 0
    state.save()
    # On failure, the next tick will re-evaluate. The watchdog
    # doesn't arm until apply succeeded, so a chronically broken
    # apply just shows as repeated apply_result=False in the
    # operator's audit log — the operator decides whether to
    # change the target.


async def _rollback(
    client: httpx.AsyncClient,
    cfg: AgentConfig,
    state: _WatchdogState,
) -> None:
    """Watchdog tripped — try to put the box back on its
    previous tag and report the result to the control plane.
    Three honest outcomes are distinguished so the operator UI
    can show what actually happened:

      * `reverted` — we had a known-good `rollback_to` and the
        revert apply succeeded. Box is back on the previous tag.
      * `revert_failed` — we had a `rollback_to` but the revert
        apply itself errored. Box is in an unknown state; an
        operator needs to look.
      * `stuck_at_broken_target` — there was no prior known-good
        tag (fresh box, first apply failed). We never tried to
        revert. The previous code reported this as `reverted`
        which was a lie that hid the failure from operators.
    """
    log(
        "warn",
        "update_agent.rollback_begin",
        failed_target=state.pending_target,
        rollback_to=state.rollback_to,
        consecutive_failures=state.failure_count,
    )
    if state.rollback_to is None:
        # No prior good tag to roll back to. Be honest about it.
        log(
            "error",
            "update_agent.rollback_no_baseline",
            failed_target=state.pending_target,
        )
        await _report_update_result(client, cfg, "stuck_at_broken_target")
    else:
        rollback_outcome = apply_update(cfg, state.rollback_to)
        log(
            "info" if rollback_outcome.ok else "error",
            "update_agent.rollback_apply_result",
            target=state.rollback_to,
            applied=rollback_outcome.ok,
        )
        result = "reverted" if rollback_outcome.ok else "revert_failed"
        await _report_update_result(client, cfg, result)

    # Reset watchdog so the next operator-set target gets a fresh
    # convergence window. A failed rollback DOES leave the box in
    # a broken state, but re-trying the same rollback every poll
    # won't help — the operator has to look.
    state.pending_target = None
    state.rollback_to = None
    state.failure_count = 0
    state.save()


def main() -> int:
    cfg = _config_from_env()
    try:
        return asyncio.run(run(cfg))
    except KeyboardInterrupt:
        log("info", "update_agent.shutdown")
        return 0


if __name__ == "__main__":
    raise SystemExit(main())
