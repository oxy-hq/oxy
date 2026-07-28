"""Clip archival to S3 on compliance violations (Tier B #6).

**Why presigned URLs and no AWS creds on the edge.** Restaurant
back rooms aren't a secure environment — staff turnover, walk-in
service techs, and cleaning crews mean we can't treat the box
as a credential vault. Holding AWS keys at the edge would leak
the bucket scope (and possibly more) to anyone with physical
access.

Instead the server holds the only AWS principal. The worker
asks for a per-clip presigned PUT URL bound to bucket + key +
content length + content type + a short TTL, then PUTs the
bytes. Even if a box is fully compromised, the worst it can do
is upload bytes to its own future report ids.

Flow:
  1. CameraReader emits a compliance report with
     `compliant=false` in the structured JSON.
  2. Before enqueuing the report, the reader hands us
     `(report_id, camera_id, segment_window)` and we:
        a. Pull the fMP4 from MTX's playback server.
        b. POST `/control/clips/sign` with content_length →
           get back `{presigned_put_url, key}`.
        c. PUT the bytes to that URL.
  3. The caller stamps `evidence_s3_key` on the outgoing report.

Failure isolation: any failure at any step logs + returns None.
The compliance report always ships either way; the S3 column on
the row just stays empty when archival didn't happen.
"""
from __future__ import annotations

from datetime import datetime, timezone, timedelta
from typing import Any

import httpx

from .log import log


# MTX playback server. The worker is co-located with MediaMTX in
# the same docker-compose so the loopback address is correct.
_MTX_PLAYBACK_URL = "http://127.0.0.1:9996/get"

# Pad the dwell window by this many seconds on each side so the
# clip shows context (operator can see the lead-up + the
# resolution). 5s is the V1 hard-coded value; expose as env if
# customers ask.
_CLIP_PAD_SECS = 5

# How long we'll wait for MTX to assemble the clip. fMP4 mux is
# cheap but a busy box can lag; 30s is generous.
_MTX_CLIP_TIMEOUT_S = 30.0

# Upload timeout — the presigned PUT is bandwidth-limited by
# whatever the box's WAN can sustain. 60s covers a few-MB clip
# even on slower restaurant uplinks; we don't retry, just log
# and move on (the compliance report ships either way).
_S3_PUT_TIMEOUT_S = 60.0


async def upload_window_clip(
    *,
    oxy_client: httpx.AsyncClient,
    report_id: str,
    camera_id: str,
    segment_start: datetime,
    segment_end: datetime,
) -> str | None:
    """Pull a time-window clip from MTX, fetch a presigned PUT URL
    from Oxy, upload the bytes, and return the bucket-relative key
    on success or None on any failure.

    Used for both compliance-violation windows and congestion
    windows — `report_id` is just the clip identifier that becomes
    the S3 key stem; the caller decides what window to capture.

    `oxy_client` is the main control-plane httpx client — it
    already has the bearer/JWT auth wired, so we don't need a
    second auth flow."""
    duration = (segment_end - segment_start).total_seconds() + 2 * _CLIP_PAD_SECS
    if duration <= 0:
        log("warn", "clip_archive.bad_window",
            report_id=report_id, duration=duration)
        return None

    # Step 1 — fetch fMP4 from MTX.
    try:
        async with httpx.AsyncClient(timeout=_MTX_CLIP_TIMEOUT_S) as client:
            resp = await client.get(
                _MTX_PLAYBACK_URL,
                # MTX 1.18+ playback requires SOMETHING in the
                # auth slot before its external-auth callback
                # runs. The values don't matter; the local Oxy
                # callback approves all non-WebRTC actions.
                auth=("oxy", "any"),
                params={
                    "path": f"cam-{camera_id.replace('-', '')}",
                    # MTX wants RFC3339 with `Z`; we render
                    # explicitly so a non-UTC tzinfo on the
                    # input doesn't accidentally drift the start.
                    "start": _to_rfc3339_z(segment_start, offset_secs=-_CLIP_PAD_SECS),
                    "duration": f"{duration:.3f}",
                    "format": "fmp4",
                },
            )
            resp.raise_for_status()
            clip_bytes = resp.content
    except Exception as e:
        log("warn", "clip_archive.mtx_fetch_failed",
            report_id=report_id, camera_id=camera_id, error=str(e))
        return None
    if not clip_bytes:
        log("warn", "clip_archive.empty_clip",
            report_id=report_id, camera_id=camera_id)
        return None

    # Step 2 — ask Oxy for a presigned PUT URL. The server's
    # config (bucket / region / key prefix / TTL) is the source
    # of truth; we just describe what we want to upload.
    try:
        sign_resp = await oxy_client.post(
            "/control/clips/sign",
            json={
                "report_id": report_id,
                "segment_start": segment_start.isoformat(),
                "content_length": len(clip_bytes),
            },
        )
        if sign_resp.status_code == 503:
            # Server says S3 isn't configured. Quiet — operator
            # opted into "no archival" by not setting
            # OXY_CAMERAS_CLIPS_S3_BUCKET; we don't need a warning per
            # violation.
            log("debug", "clip_archive.disabled_server_side",
                report_id=report_id)
            return None
        sign_resp.raise_for_status()
        sign = sign_resp.json()
    except Exception as e:
        log("warn", "clip_archive.sign_failed",
            report_id=report_id, error=str(e))
        return None

    presigned_url = sign.get("presigned_put_url")
    key = sign.get("key")
    if not presigned_url or not key:
        log("warn", "clip_archive.sign_invalid_response",
            report_id=report_id, sign=sign)
        return None

    # Step 3 — upload. The presign bound `Content-Type` and
    # `Content-Length` server-side; the headers we send have to
    # match exactly or S3 rejects with SignatureDoesNotMatch.
    try:
        async with httpx.AsyncClient(timeout=_S3_PUT_TIMEOUT_S) as put_client:
            put_resp = await put_client.put(
                presigned_url,
                content=clip_bytes,
                headers={
                    "Content-Type": "video/mp4",
                    "Content-Length": str(len(clip_bytes)),
                },
            )
            put_resp.raise_for_status()
    except Exception as e:
        log("warn", "clip_archive.s3_put_failed",
            report_id=report_id, key=key, error=str(e))
        return None

    log("info", "clip_archive.uploaded",
        report_id=report_id, key=key, bytes=len(clip_bytes))
    return key


def _to_rfc3339_z(ts: datetime, *, offset_secs: float = 0.0) -> str:
    """RFC3339 with `Z` suffix, offsetable by `offset_secs`. MTX
    wants this format on its `start=` query param; trailing `Z`
    is required even for naive datetimes (we coerce to UTC)."""
    if ts.tzinfo is None:
        ts = ts.replace(tzinfo=timezone.utc)
    shifted = ts + timedelta(seconds=offset_secs)
    return shifted.astimezone(timezone.utc).strftime("%Y-%m-%dT%H:%M:%S.%fZ")[:-4] + "Z"
