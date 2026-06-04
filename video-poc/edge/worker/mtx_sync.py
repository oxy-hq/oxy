"""Reconcile MediaMTX's path config against the camera list Oxy
returned for this edge box.

## Naming

Path name is `cam-{camera_id_hex}` (UUID without hyphens). This name
is the contract with the Oxy HLS proxy in
`crates/cameras/src/service/preview.rs` — change here MUST change
there too.

## Reconciliation

Compute three sets:
  - **add**: cameras in our config whose path isn't yet in MTX
  - **keep**: cameras in our config whose path matches (correct
    source URL)
  - **remove**: paths in MTX named `cam-*` that aren't in our config
    (covers cameras unbound or deleted in Oxy)

We don't touch paths that don't start with `cam-` — the MTX config
file may carry hand-managed paths (e.g. the existing
`cam-protect-01` demo path) that we'd be wrong to delete.

## What we DON'T sync

- `analytics_consent = false` cameras: still registered with MTX so
  preview works, but the worker drops their events. This is the
  least surprising posture — disabling consent shouldn't break the
  operator's "show me the camera" page.
- Cameras with `rtsp_url is None` (inventory imported but RTSP
  alias not fetched yet): skip; MTX has nothing to pull from.

## Failure posture

MTX API errors are logged, not fatal. The worker keeps running with
whatever paths MTX already has. Next config poll re-attempts. The
goal is "live preview eventually shows up" not "kill the worker if
MTX is having a moment."
"""
from __future__ import annotations

import re
from typing import Any, Iterable
from uuid import UUID

from .config import CameraConfig
from .log import log
from .mtx_client import MtxApiError, MtxClient

# Auto-managed paths have the exact shape `cam-<32 lowercase hex>` —
# the UUID `simple()` form Rust uses on the proxy side. Anything that
# starts with `cam-` but isn't a 32-hex UUID (like `cam-01`,
# `cam-protect-01` from the sim config) is hand-authored and MUST NOT
# be touched by the reconciler.
MANAGED_PATH_PREFIX = "cam-"
_MANAGED_PATH_RE = re.compile(r"^cam-[0-9a-f]{32}$")


def is_managed_path(name: str) -> bool:
    """True iff `name` matches our auto-generated `cam-<uuid>` pattern.
    Anything else — including hand-authored paths like `cam-01` — is
    out of scope for the reconciler.
    """
    return bool(_MANAGED_PATH_RE.match(name))


def path_name_for(camera_id: UUID) -> str:
    """Stable, hyphen-free path name. Matches the Rust proxy's
    `cam-{camera_id.simple()}` template.
    """
    return f"{MANAGED_PATH_PREFIX}{camera_id.hex}"


def _path_config_for(rtsp_url: str) -> dict[str, Any]:
    """Build the MTX v3 path config dict that pulls from `rtsp_url`.

    Uses ffmpeg-as-source (`source: publisher` + `runOnInit`) rather
    than MediaMTX's native RTSP source pull. UniFi Protect feeds
    speak MTAP-16 H.264 packetization (RFC 6184), and MTX's built-in
    depacketizer doesn't handle it — native pull fails with
    "unsupported packet". ffmpeg's RTSP demuxer is happy with it, so
    we let ffmpeg pull from the camera and republish locally to a
    plain RTSP path that MTX then exposes as HLS.

    This is the same trick the hand-authored `cam-protect-01` uses in
    `central/mediamtx/mediamtx.yml`. The auto-managed paths follow
    the same template so the UniFi case "just works." Sim feeds
    (already plain RTSP from MTX's own runOnInit) work too with one
    extra local hop; the cost is negligible.

    Flags chosen:
      - `-rtsp_transport tcp`  — UniFi requires it (UDP drops too many
                                  packets through their NAT).
      - `-c:v copy`            — passthrough, zero re-encode cost.
      - `-an`                  — drop audio. UniFi sends AAC with an
                                  AU-Index format MTX's depacketizer
                                  also can't handle, and we don't need
                                  audio for analytics.
      - `-fflags +discardcorrupt` etc. — survive occasional packet
                                          corruption without dying.
    """
    return {
        "source": "publisher",
        "runOnInit": (
            "ffmpeg -nostdin -hide_banner -loglevel warning "
            "-rtsp_transport tcp "
            "-fflags +discardcorrupt -err_detect ignore_err "
            f"-i {rtsp_url} "
            "-map 0:v:0 -c:v copy -an "
            "-f rtsp -rtsp_transport tcp rtsp://localhost:$RTSP_PORT/$MTX_PATH"
        ),
        "runOnInitRestart": True,
    }


def _desired_paths(cameras: Iterable[CameraConfig]) -> dict[str, dict[str, Any]]:
    """Map of `{path_name: mtx_path_config}` for cameras MTX should serve."""
    out: dict[str, dict[str, Any]] = {}
    for cam in cameras:
        if not cam.rtsp_url:
            continue
        out[path_name_for(cam.id)] = _path_config_for(cam.rtsp_url)
    return out


async def reconcile(client: MtxClient, cameras: Iterable[CameraConfig]) -> None:
    """Bring MTX's path config in line with `cameras`. Best-effort:
    surfaces a per-operation log on failure but doesn't raise — the
    next sync tick will retry.
    """
    desired = _desired_paths(cameras)

    try:
        current = await client.list_paths()
    except MtxApiError as e:
        log(
            "warn",
            "mtx.sync_skipped",
            error=str(e),
            note="will retry on the next config poll",
        )
        return

    current_managed: set[str] = {
        p["name"]
        for p in current
        if isinstance(p.get("name"), str) and is_managed_path(p["name"])
    }
    desired_names = set(desired.keys())

    # Add missing paths. We don't try to detect drift on `keep` here —
    # if MTX has a path but the source URL differs from what we want
    # (e.g. camera's rtsp_url changed), `add` will 409 and we'd need
    # delete-then-add. Acceptable for v1: source URLs are stable after
    # UniFi import; if they change we can flip the flag below to true.
    REPLACE_ON_DRIFT = False  # set to True if source URLs start drifting

    for name in sorted(desired_names - current_managed):
        try:
            await client.add_path(name, desired[name])
        except MtxApiError as e:
            log("warn", "mtx.path_add_failed", name=name, error=str(e))

    if REPLACE_ON_DRIFT:
        # With the ffmpeg-as-source shape we can no longer cheaply
        # diff "source URL" against MTX's stored config because the
        # URL is embedded in the runOnInit command string. If drift
        # detection ever becomes necessary, the simplest path is to
        # always delete+add managed paths on every reconcile (idempotent
        # but a hair heavier). Out of scope for the v1 flag.
        log("warn", "mtx.drift_detection_disabled", note="see comment in mtx_sync.py")

    # Remove paths for cameras we no longer manage. Only touch
    # things under our prefix; hand-managed paths in mediamtx.yml
    # stay untouched.
    for name in sorted(current_managed - desired_names):
        try:
            await client.delete_path(name)
        except MtxApiError as e:
            log("warn", "mtx.path_delete_failed", name=name, error=str(e))

    log(
        "info",
        "mtx.sync_complete",
        added=len(desired_names - current_managed),
        kept=len(desired_names & current_managed),
        removed=len(current_managed - desired_names),
    )
