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
            r = await client.post(f"/control/boxes/{box_id}/health", json=payload)
            r.raise_for_status()
        except Exception as e:
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
            r = await client.post(f"/control/cameras/{camera_id}/health", json=payload)
            r.raise_for_status()
        except Exception as e:
            log("warn", "health.camera_push_failed", camera_id=str(camera_id), error=str(e))
        await asyncio.sleep(interval_s)
