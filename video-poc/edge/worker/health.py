"""Box-level + camera-level health reporters."""
from __future__ import annotations

import asyncio
import json
import os
import time
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Callable
from uuid import UUID

import httpx
import psutil

from .log import log


_BOOT_TIME = time.time()

# Compatibility manifest baked into the image at build time
# (Dockerfile + .github/workflows/edge-image.yaml). Read once at
# startup and sent in every box-health POST so the server can
# show `edge_version` in the EdgeBoxesTable and (future) reject
# too-old workers on protocol_version mismatch.
_COMPATIBILITY_PATH = Path(
    os.environ.get("OXY_COMPATIBILITY_PATH", "/opt/oxy-edge/compatibility.json")
)


def _load_compatibility() -> dict[str, Any] | None:
    """Read /opt/oxy-edge/compatibility.json once. Missing or invalid
    file = None — older images built before this plumbing existed
    won't have it, and the server's column is nullable. We never
    raise: an absent manifest must not block health reporting."""
    try:
        raw = _COMPATIBILITY_PATH.read_text()
    except (FileNotFoundError, PermissionError):
        return None
    try:
        data = json.loads(raw)
    except json.JSONDecodeError as exc:
        log("warn", "health.compatibility_parse_failed", error=str(exc))
        return None
    if not isinstance(data, dict):
        return None
    return data


_COMPATIBILITY = _load_compatibility()


# Statuses we treat as transient — usually an ALB target hiccup, a
# brief pgwire pool exhaustion on Oxy, or a network reset. Bounded
# retry recovers the sample without dropping it. Auth / validation
# errors (4xx) fall through immediately — retry won't help and
# would just spam the logs.
_TRANSIENT_STATUSES = frozenset({502, 503, 504})


async def _post_with_retry(
    client: httpx.AsyncClient,
    path: str,
    payload: dict,
    op: str,
    **extra_log: object,
) -> None:
    """POST with bounded retry on transient 5xx + network errors.

    Three attempts, 1s -> 2s -> 4s backoff. Total worst-case wall
    clock ~7s, comfortably inside the loop's interval. Raises the
    last exception so the caller's existing failure log fires for
    persistent issues.
    """
    backoff = 1.0
    last_err: Exception | None = None
    for attempt in range(3):
        try:
            r = await client.post(path, json=payload)
            r.raise_for_status()
            if attempt > 0:
                log("info", f"{op}_recovered", attempts=attempt + 1, **extra_log)
            return
        except httpx.HTTPStatusError as e:
            last_err = e
            if e.response.status_code not in _TRANSIENT_STATUSES:
                break  # 4xx / non-transient 5xx — retry won't help
        except httpx.RequestError as e:
            last_err = e  # connection reset, DNS hiccup, etc.
        if attempt < 2:
            await asyncio.sleep(backoff)
            backoff *= 2
    if last_err is not None:
        raise last_err
if _COMPATIBILITY:
    log(
        "info",
        "health.compatibility_loaded",
        edge_version=_COMPATIBILITY.get("edge_version"),
        protocol_version=_COMPATIBILITY.get("protocol_version"),
    )


def _now() -> datetime:
    return datetime.now(timezone.utc)


async def box_health_loop(
    client: httpx.AsyncClient,
    box_id: UUID,
    interval_s: int,
    image_tag: str,
) -> None:
    while True:
        try:
            payload = {
                "ts": _now().isoformat(),
                "cpu_pct": psutil.cpu_percent(interval=None),
                "gpu_pct": None,  # no GPU in sim mode
                "mem_pct": psutil.virtual_memory().percent,
                "temp_c": None,
                "uptime_s": int(time.time() - _BOOT_TIME),
                "image_tag": image_tag,
                "containers_running": 1,
            }
            if _COMPATIBILITY is not None:
                payload["compatibility"] = _COMPATIBILITY
            await _post_with_retry(
                client, f"/control/boxes/{box_id}/health", payload, "health.box_push"
            )
        except Exception as e:
            # Final-failure log (after retries inside _post_with_retry).
            log("warn", "health.box_push_failed", error=str(e))
        await asyncio.sleep(interval_s)


async def camera_health_loop(
    client: httpx.AsyncClient,
    camera_id: UUID,
    interval_s: int,
    counter: Callable[[], dict[str, float | int | None]],
) -> None:
    while True:
        try:
            stats = counter()
            payload = {
                "ts": _now().isoformat(),
                "fps": stats.get("fps"),
                "bitrate_kbps": stats.get("bitrate_kbps"),
                "last_frame_at": stats.get("last_frame_at"),
                "decoder_errors": int(stats.get("decoder_errors") or 0),
                "reconnect_count": int(stats.get("reconnect_count") or 0),
            }
            await _post_with_retry(
                client,
                f"/control/cameras/{camera_id}/health",
                payload,
                "health.camera_push",
                camera_id=str(camera_id),
            )
        except Exception as e:
            log("warn", "health.camera_push_failed", camera_id=str(camera_id), error=str(e))
        await asyncio.sleep(interval_s)


async def audio_health_loop(
    camera_id: UUID,
    name: str,
    interval_s: int,
    counter: Callable[[], dict[str, Any]],
) -> None:
    """Periodic liveness/stats for an AudioReader (upsell detection).

    There is no dedicated server health endpoint for audio yet, so this
    ships structured stats through the log channel (log_shipper -> control
    plane -> ops log viewer): grep `upsell.health`. Cadence mirrors
    camera_health_loop so a stuck reader (windows not advancing) or one
    that keeps erroring is visible without source-diving. When a structured
    audio-health endpoint lands, swap the log() for a _post_with_retry.
    """
    while True:
        try:
            s = counter()
            log(
                "info",
                "upsell.health",
                camera_id=str(camera_id),
                name=name,
                windows=s.get("windows"),
                attempts=s.get("attempts"),
                errors=s.get("errors"),
                last_text_at=s.get("last_text_at"),
            )
        except Exception as e:  # noqa: BLE001 — never kill the loop
            log("warn", "upsell.health_failed", camera_id=str(camera_id), error=str(e))
        await asyncio.sleep(interval_s)
