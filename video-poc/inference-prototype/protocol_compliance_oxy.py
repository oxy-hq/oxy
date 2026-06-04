"""Protocol-compliance pipeline wired to the control-api.

Canonical "edge worker logic" prototype: same two-stage YOLO + VLM shape as
the original dataframehq/video-lm notebook, but:

- Input is a local video file (will switch to RTSP when lifted into edge/worker/).
- Per-frame zone-enter / zone-exit / line-cross events go through OxyEmitter
  to central Postgres, not local CSV.
- VLM calls are **on-trigger only** (sustained presence in PPE zone), not on
  a fixed segment cadence. Default model is Claude Haiku.
- Compliance reports land in `camera_compliance_reports`.

Run (after `cd video-poc && make up` in another shell):
    cd video-poc/inference-prototype
    python -m venv .venv && source .venv/bin/activate
    pip install -r requirements.txt
    cp .env.example .env   # then set ANTHROPIC_API_KEY
    python protocol_compliance_oxy.py --video /path/to/your.mp4

Pre-reqs:
    - video-poc central stack is up (postgres + mediamtx + control-api).
    - init.sql seeded the 'video-lm-test' camera + 'video-lm-notebook' edge box.
    - ANTHROPIC_API_KEY is set.

This script will be split into modules and grafted into video-poc/edge/worker/
once the behavior is validated end-to-end on real footage.
"""
from __future__ import annotations

import argparse
import base64
import os
import sys
import time
from collections import defaultdict
from dataclasses import dataclass, field
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

import anthropic
import cv2
import numpy as np
import supervision as sv
from dotenv import load_dotenv
from ultralytics import YOLO

from oxy_emit import OxyEmitter


# ---------------------------------------------------------------------------
# Config
# ---------------------------------------------------------------------------

load_dotenv()

YOLO_MODEL              = os.getenv("YOLO_MODEL", "yolo11n.pt")
VLM_MODEL               = os.getenv("VLM_MODEL", "claude-haiku-4-5-20251001")
CAMERA_NAME             = os.getenv("CAMERA_NAME", "video-lm-test")
PERSON_CLASS_ID         = 0  # COCO person class
# Tracker. 'botsort.yaml' is more stable than 'bytetrack.yaml' on wide-angle
# / fisheye footage (less ID churn). Ultralytics ships both built-in.
TRACKER                 = os.getenv("TRACKER", "botsort.yaml")
TRIGGER_DWELL_SEC       = float(os.getenv("TRIGGER_DWELL_SEC", "4"))
TRACK_COOLDOWN_SEC      = float(os.getenv("TRACK_COOLDOWN_SEC", "300"))
GLOBAL_MIN_INTERVAL_SEC = float(os.getenv("GLOBAL_MIN_INTERVAL_SEC", "60"))
DWELL_EVENT_EVERY_SEC   = float(os.getenv("DWELL_EVENT_EVERY_SEC", "30"))

REQUIRED_ATTIRE  = set(filter(None, os.getenv("REQUIRED_ATTIRE",  "hat,apron").split(",")))
REQUIRED_HYGIENE = set(filter(None, os.getenv("REQUIRED_HYGIENE", "glove").split(",")))


# ---------------------------------------------------------------------------
# Per-track state
# ---------------------------------------------------------------------------

@dataclass
class TrackState:
    in_zone_since: datetime | None = None
    last_dwell_emit: datetime | None = None
    last_vlm_check: datetime | None = None
    last_seen_classes: set[str] = field(default_factory=set)


# ---------------------------------------------------------------------------
# VLM
# ---------------------------------------------------------------------------

VLM_PROMPT_TEMPLATE = """\
You are a site safety auditor looking at one CCTV frame.
A computer vision pipeline has detected a worker (tracker_id={track_id}) holding the
PPE zone for {dwell_seconds:.0f} seconds. Decide whether they appear to be following
the required protocols.

Required attire items:  {attire}
Required hygiene items: {hygiene}

Look at the frame and return ONLY valid JSON with this shape:
{{
  "attire_compliant":  true | false,
  "hygiene_compliant": true | false,
  "missing_items":     ["hat", "apron", ...],
  "confidence":        0.0 .. 1.0,
  "notes":             "one short sentence with the visual evidence"
}}

Be specific. If you cannot see the worker clearly, say so in `notes` and lower
`confidence` accordingly — do not invent findings.
"""


def call_vlm(
    client: anthropic.Anthropic,
    frame: np.ndarray,
    track_id: int,
    dwell_seconds: float,
) -> tuple[str, dict[str, Any] | None, int | None]:
    """Send one frame + context to the VLM. Returns (raw_text, parsed_json, tokens).

    On parse failure, parsed_json is None and raw_text still has the model's reply.
    """
    _, buf = cv2.imencode(".jpg", frame, [cv2.IMWRITE_JPEG_QUALITY, 85])
    img_b64 = base64.b64encode(buf).decode()
    prompt = VLM_PROMPT_TEMPLATE.format(
        track_id=track_id,
        dwell_seconds=dwell_seconds,
        attire=", ".join(sorted(REQUIRED_ATTIRE)) or "(none)",
        hygiene=", ".join(sorted(REQUIRED_HYGIENE)) or "(none)",
    )
    resp = client.messages.create(
        model=VLM_MODEL,
        max_tokens=400,
        messages=[{
            "role": "user",
            "content": [
                {"type": "image", "source": {
                    "type": "base64", "media_type": "image/jpeg", "data": img_b64,
                }},
                {"type": "text", "text": prompt},
            ],
        }],
    )
    raw = resp.content[0].text
    tokens = (resp.usage.input_tokens or 0) + (resp.usage.output_tokens or 0)

    import json as _json
    import re as _re
    match = _re.search(r"\{.*\}", raw, _re.DOTALL)
    parsed: dict[str, Any] | None = None
    if match:
        try:
            parsed = _json.loads(match.group())
        except _json.JSONDecodeError:
            parsed = None
    return raw, parsed, tokens


# ---------------------------------------------------------------------------
# Pipeline
# ---------------------------------------------------------------------------

def run(video_path: Path) -> None:
    print(f"[setup] video={video_path}")

    info = sv.VideoInfo.from_video_path(str(video_path))
    fps = info.fps
    print(f"[setup] {info.width}x{info.height} @ {fps}fps, {info.total_frames} frames")

    emitter = OxyEmitter()
    emitter.register()
    if emitter.box_id is None:
        print("[setup] registration returned no box_id", file=sys.stderr)
        return
    cam = emitter.get_camera(CAMERA_NAME)
    print(f"[setup] camera_id={cam.id} ({cam.name})")
    print(f"[setup] zones: {[z.get('zone_id') for z in cam.zones_json]}")
    print(f"[setup] lines: {[l.get('line_id') for l in cam.lines_json]}")

    # ------------------------------------------------------------------
    # Build supervision PolygonZone / LineZone instances from cam config
    # ------------------------------------------------------------------
    zones: dict[str, sv.PolygonZone] = {}
    for z in cam.zones_json:
        zones[z["zone_id"]] = sv.PolygonZone(polygon=np.array(z["polygon"]))

    lines: dict[str, sv.LineZone] = {}
    for l in cam.lines_json:
        lines[l["line_id"]] = sv.LineZone(
            start=sv.Point(*l["p1"]),
            end=sv.Point(*l["p2"]),
        )

    if not zones and not lines:
        print(
            "[setup] no zones or lines configured for this camera. "
            "Patch cameras.zones_json/lines_json or run the calibration helper.",
            file=sys.stderr,
        )
        return

    # ------------------------------------------------------------------
    # Models
    # ------------------------------------------------------------------
    # Tracking runs inside Ultralytics' model.track() so we can use BoT-SORT
    # (better than ByteTrack on fisheye/wide-angle). track_ids land in the
    # results object and supervision picks them up automatically.
    model = YOLO(YOLO_MODEL)
    smoother = sv.DetectionsSmoother()
    vlm_client = anthropic.Anthropic()
    print(f"[setup] tracker={TRACKER} trigger_dwell={TRIGGER_DWELL_SEC}s "
          f"track_cooldown={TRACK_COOLDOWN_SEC}s global_min={GLOBAL_MIN_INTERVAL_SEC}s")

    # ------------------------------------------------------------------
    # Per-track state, per (zone_id, track_id) pair
    # ------------------------------------------------------------------
    track_state: dict[tuple[str, int], TrackState] = defaultdict(TrackState)
    last_global_vlm: datetime | None = None
    in_zone_now: dict[str, set[int]] = defaultdict(set)  # zone_id -> tracks present last frame
    line_crossed_total = defaultdict(lambda: [0, 0])  # line_id -> [in, out]

    vlm_calls = 0
    frame_idx = 0
    started_at = datetime.now(timezone.utc)

    for frame in sv.get_video_frames_generator(str(video_path)):
        # Wall-clock timestamp for this frame (matches what the edge worker
        # would stamp from the RTSP arrival time).
        frame_ts = started_at.fromtimestamp(
            started_at.timestamp() + (frame_idx / fps),
            tz=timezone.utc,
        )

        # ---- Detect + track --------------------------------------------------
        # model.track() with persist=True maintains tracker state across calls.
        # Filtering to person class at the source is cheaper than post-filter.
        results = model.track(
            frame,
            persist=True,
            tracker=TRACKER,
            classes=[PERSON_CLASS_ID],
            verbose=False,
        )[0]
        detections = sv.Detections.from_ultralytics(results)
        detections = smoother.update_with_detections(detections)

        tids = (
            detections.tracker_id.tolist()
            if detections.tracker_id is not None
            else []
        )

        # ---- Zone enter/exit transitions -------------------------------------
        for zone_id, zone in zones.items():
            mask = zone.trigger(detections=detections)
            present_now = {tids[i] for i in range(len(tids)) if mask[i]}
            present_prev = in_zone_now[zone_id]

            entered = present_now - present_prev
            exited = present_prev - present_now

            for tid in entered:
                emitter.emit_event(
                    cam.id, "enter", track_id=str(tid),
                    ts=frame_ts, zone_id=zone_id,
                )
                track_state[(zone_id, tid)].in_zone_since = frame_ts

            for tid in exited:
                state = track_state[(zone_id, tid)]
                dwell = (
                    (frame_ts - state.in_zone_since).total_seconds()
                    if state.in_zone_since else None
                )
                emitter.emit_event(
                    cam.id, "exit", track_id=str(tid),
                    ts=frame_ts, zone_id=zone_id, dwell_seconds=dwell,
                )
                state.in_zone_since = None
                state.last_dwell_emit = None

            in_zone_now[zone_id] = present_now

            # Periodic dwell update so analytics has live "still in zone" signal.
            for tid in present_now:
                state = track_state[(zone_id, tid)]
                if state.in_zone_since is None:
                    continue
                if (
                    state.last_dwell_emit is None
                    or (frame_ts - state.last_dwell_emit).total_seconds() >= DWELL_EVENT_EVERY_SEC
                ):
                    dwell = (frame_ts - state.in_zone_since).total_seconds()
                    emitter.emit_event(
                        cam.id, "dwell", track_id=str(tid),
                        ts=frame_ts, zone_id=zone_id, dwell_seconds=dwell,
                    )
                    state.last_dwell_emit = frame_ts

        # ---- Line crossings --------------------------------------------------
        for line_id, line in lines.items():
            crossed_in, crossed_out = line.trigger(detections)
            for i, (ci, co) in enumerate(zip(crossed_in, crossed_out)):
                if not (ci or co) or i >= len(tids):
                    continue
                tid = tids[i]
                emitter.emit_event(
                    cam.id, "line_cross", track_id=str(tid),
                    ts=frame_ts, line_id=line_id,
                    confidence=1.0 if ci else 0.0,  # +/- direction encoded in confidence for now
                )
                line_crossed_total[line_id][0 if ci else 1] += 1

        # ---- On-trigger VLM check --------------------------------------------
        for (zone_id, tid), state in list(track_state.items()):
            if state.in_zone_since is None:
                continue
            dwell = (frame_ts - state.in_zone_since).total_seconds()
            if dwell < TRIGGER_DWELL_SEC:
                continue
            if (
                state.last_vlm_check is not None
                and (frame_ts - state.last_vlm_check).total_seconds() < TRACK_COOLDOWN_SEC
            ):
                continue
            if (
                last_global_vlm is not None
                and (frame_ts - last_global_vlm).total_seconds() < GLOBAL_MIN_INTERVAL_SEC
            ):
                continue

            print(f"[vlm] zone={zone_id} track={tid} dwell={dwell:.1f}s — calling {VLM_MODEL}")
            try:
                raw, parsed, tokens = call_vlm(vlm_client, frame, tid, dwell)
            except anthropic.APIError as exc:
                print(f"[vlm] api error: {exc}", file=sys.stderr)
                # Don't update cooldowns on hard failure — let the next pass retry.
                continue

            emitter.emit_compliance_report(
                cam.id,
                segment_start=state.in_zone_since,
                segment_end=frame_ts,
                trigger_type="sustained_presence",
                trigger_track_id=str(tid),
                vlm_model=VLM_MODEL,
                report_text=raw,
                structured_json=parsed or {},
                tokens_used=tokens,
            )
            state.last_vlm_check = frame_ts
            last_global_vlm = frame_ts
            vlm_calls += 1

        if frame_idx % 100 == 0:
            print(
                f"[frame {frame_idx}/{info.total_frames}] "
                f"persons={len(detections)} "
                f"zones={ {z: len(in_zone_now[z]) for z in zones} } "
                f"vlm_calls={vlm_calls}"
            )

        frame_idx += 1

    emitter.flush()
    emitter.close()

    elapsed = time.monotonic()
    print(
        f"\n[done] frames={frame_idx} vlm_calls={vlm_calls} "
        f"line_crossings={dict(line_crossed_total)}"
    )


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------

def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument(
        "--video",
        type=Path,
        default=None,
        help="path to local MP4 (defaults to VIDEO_PATH env var; will switch to RTSP in the edge worker)",
    )
    args = ap.parse_args()

    # Env-var fallback runs AFTER parse_args so it doesn't depend on argparse
    # evaluating defaults at the right point in load_dotenv()'s lifecycle.
    if args.video is None:
        env_video = os.environ.get("VIDEO_PATH")
        if not env_video:
            print("--video is required (or set VIDEO_PATH in .env)", file=sys.stderr)
            sys.exit(2)
        args.video = Path(env_video)

    if not args.video.exists():
        print(f"video not found: {args.video}", file=sys.stderr)
        sys.exit(1)
    if not os.getenv("ANTHROPIC_API_KEY"):
        print("ANTHROPIC_API_KEY not set (copy .env.example to .env)", file=sys.stderr)
        sys.exit(1)

    run(args.video)


if __name__ == "__main__":
    main()
