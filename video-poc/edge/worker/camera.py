"""Per-camera frame reader + inference.

Lifted from `video-poc/inference-prototype/protocol_compliance_oxy.py` after
tuning on real Protect footage:

  Per-frame (cheap, ~10 fps):
    YOLO11n -> Ultralytics model.track(tracker='bytetrack.yaml') -> supervision
    PolygonZone / LineZone triggers -> camera_events (enter/exit/dwell/line_cross)

  On-trigger (expensive, sparse, ~$0.002 per call):
    When a track sustains the PPE zone for >= TRIGGER_DWELL_SEC, dispatch a
    Claude Haiku check to the asyncio main loop (so the frame thread doesn't
    block on the 2-5s API roundtrip). Per-track cooldown + per-camera global
    cooldown bound the call rate.

The reader runs in a worker thread (OpenCV is blocking). Events go onto an
asyncio.Queue consumed from the main loop. VLM calls are scheduled via
asyncio.run_coroutine_threadsafe; results enqueue back through the queue as
'compliance_report' payloads, then through the outbox to /control/compliance-reports.
"""
from __future__ import annotations

import asyncio
import base64
import json
import os
import re
import threading
import time
from collections import defaultdict
from dataclasses import dataclass, field
from datetime import datetime, timedelta, timezone
from typing import Any
from uuid import UUID, uuid4

import anthropic
import cv2  # type: ignore[import-untyped]
import httpx
import numpy as np
import supervision as sv  # type: ignore[import-untyped]
from ultralytics import YOLO  # type: ignore[import-untyped]

from . import ppe_yolo
from .clip_archive import upload_window_clip
from .config import CameraConfig
from .congestion import CONGESTION_CLIP_SEC, CongestionDetector, CongestionFlag
from .log import log
from .prompts import render_prompt

# ---------------------------------------------------------------------------
# Tunables (env-overridable). Defaults match the tuned values from the
# inference-prototype run against real Protect footage.
# ---------------------------------------------------------------------------

YOLO_MODEL              = os.environ.get("YOLO_MODEL", "yolo11n.pt")
# ByteTrack, not BoT-SORT. Cameras are fixed, so BoT-SORT's camera-motion
# compensation is wasted work and its ReID appearance model is extra cost per
# frame — both matter on a CPU/edge box running several streams. ByteTrack is the
# lighter association-only tracker and is what the fleet design specifies. Track
# identity is best-effort anyway: the occupancy metric sums exit dwell_seconds,
# so it's robust to id churn regardless of tracker (see the time-to-serve
# investigation). Override with TRACKER=botsort.yaml if a camera ever pans.
TRACKER                 = os.environ.get("TRACKER", "bytetrack.yaml")
VLM_MODEL               = os.environ.get("VLM_MODEL", "claude-haiku-4-5-20251001")
TRIGGER_DWELL_SEC       = float(os.environ.get("TRIGGER_DWELL_SEC", "4"))
TRACK_COOLDOWN_SEC      = float(os.environ.get("TRACK_COOLDOWN_SEC", "300"))
GLOBAL_MIN_INTERVAL_SEC = float(os.environ.get("GLOBAL_MIN_INTERVAL_SEC", "60"))
DWELL_EVENT_EVERY_SEC   = float(os.environ.get("DWELL_EVENT_EVERY_SEC", "30"))
PERSON_CLASS_ID = 0  # COCO person

# Synthetic zone identifier used when a camera has no operator-drawn
# polygons. Treats the whole frame as one implicit zone so a fresh
# deployment (or any camera the operator hasn't gotten around to
# configuring yet) still produces compliance signal.
#
# The leading underscore is the marker: any user-drawn zone goes
# through the UI which generates UUID-ish ids, never something
# starting with `_`. Lets downstream analytics filter system zones
# vs user zones with a simple prefix check.
WHOLE_FRAME_ZONE_ID = "_whole_frame"


# Default fallback. Real prompts live in prompts.py keyed by camera
# role; we pull the template per-VLM call so the operator's role tag
# is reflected on the very next compliance check.


# ---------------------------------------------------------------------------
# Per-track state
# ---------------------------------------------------------------------------

def _is_violation(parsed: dict[str, Any] | None) -> bool:
    """True when the VLM output flags a compliance violation worth
    archiving. The structured JSON shape mirrors what the kitchen
    starter pack prompts produce:

        { "attire_compliant": bool, "presence_compliant": bool,
          "confidence": float, ... }

    We require BOTH (a) at least one `*_compliant: False` flag AND
    (b) confidence ≥ 0.5 — same threshold the backend SQL filter
    uses to suppress empty-frame false positives. Below threshold,
    we treat the VLM as uncertain and skip the upload to keep S3
    cost in line with operator-visible incidents.
    """
    if not parsed:
        return False
    try:
        confidence = float(parsed.get("confidence", 0.0))
    except (TypeError, ValueError):
        confidence = 0.0
    if confidence < 0.5:
        return False
    flags = (
        parsed.get("attire_compliant"),
        parsed.get("presence_compliant"),
    )
    return any(f is False for f in flags)


@dataclass


class TrackState:
    in_zone_since: datetime | None = None
    last_dwell_emit: datetime | None = None
    last_vlm_check: datetime | None = None


# ---------------------------------------------------------------------------
# Camera reader
# ---------------------------------------------------------------------------

class CameraReader:
    def __init__(
        self,
        cfg: CameraConfig,
        loop: asyncio.AbstractEventLoop,
        queue: asyncio.Queue,
        vlm_client: anthropic.AsyncAnthropic | None,
        # Tier B #6 — control-plane httpx client used to request
        # presigned S3 PUT URLs for violation clips. Optional;
        # when None, the reader skips archival and ships
        # compliance reports without `evidence_s3_key`. Passed
        # in by main.py so we reuse the same auth chain (JWT or
        # bearer) that the other /control/* calls go through.
        oxy_client: "httpx.AsyncClient | None" = None,
    ) -> None:
        self.cfg = cfg
        self.loop = loop
        self.queue = queue
        self.vlm_client = vlm_client
        self._oxy_client = oxy_client
        self._stop = threading.Event()
        self._stats_lock = threading.Lock()
        self._frames = 0
        self._last_frame_at: datetime | None = None
        self._decoder_errors = 0
        self._reconnects = 0
        # Most recent decoded BGR frame, kept under `_stats_lock` for
        # the preview HTTP server to read on demand. Encoded to JPEG
        # only when something asks for it — keeping a decoded ndarray
        # is ~10x cheaper memory-wise than a pre-encoded JPEG buffer
        # at 1080p and avoids encode work for cameras nobody is
        # previewing.
        self._latest_frame: np.ndarray | None = None

        # Per-instance model (Ultralytics model.track keeps tracker state on
        # the model object — so we cannot share one model across cameras).
        self._model = YOLO(YOLO_MODEL)
        self._smoother = sv.DetectionsSmoother()

        # Zones / lines from camera config.
        self._zones: dict[str, sv.PolygonZone] = {}
        for z in cfg.zones_json:
            # Count by box CENTER, not supervision's default BOTTOM_CENTER
            # (feet). Behind a counter, feet are occluded so a feet-anchor
            # lands at/below the zone edge and the person is missed even when
            # plainly inside the zone; center is robust to that. This feed
            # drives the congestion head count below, so annotate.py uses the
            # same anchor to keep the evidence clip consistent with the count.
            self._zones[z["zone_id"]] = sv.PolygonZone(
                polygon=np.array(z["polygon"]),
                triggering_anchors=(sv.Position.CENTER,),
            )
        self._lines: dict[str, sv.LineZone] = {}
        for ln in cfg.lines_json:
            self._lines[ln["line_id"]] = sv.LineZone(
                start=sv.Point(*ln["p1"]),
                end=sv.Point(*ln["p2"]),
            )

        # Per-track state, indexed by (zone_id, track_id).
        self._track_state: dict[tuple[str, int], TrackState] = defaultdict(TrackState)
        # Last frame's in-zone set per zone (for enter/exit edge detection).
        self._in_zone_prev: dict[str, set[int]] = defaultdict(set)
        # Per-camera global cooldown timestamp.
        self._last_global_vlm: datetime | None = None
        # Sustained-occupancy detector → evidence clips on backed-up windows.
        # Pure state machine (see congestion.py); we feed it the per-zone head
        # count each frame and archive a clip when it flags.
        self._congestion = CongestionDetector()

    # ----- public api -----

    def start(self) -> threading.Thread:
        t = threading.Thread(target=self._run, name=f"cam-{self.cfg.name}", daemon=True)
        t.start()
        return t

    def stop(self) -> None:
        self._stop.set()

    def stats(self) -> dict[str, Any]:
        with self._stats_lock:
            return {
                "fps": float(self._frames) / 30.0,  # frames-per-30s window approximation
                "bitrate_kbps": None,
                "last_frame_at": self._last_frame_at.isoformat() if self._last_frame_at else None,
                "decoder_errors": self._decoder_errors,
                "reconnect_count": self._reconnects,
            }

    def reset_stats_window(self) -> None:
        with self._stats_lock:
            self._frames = 0

    def latest_jpeg(self, quality: int = 70) -> bytes | None:
        """Encode the most recent decoded frame as JPEG. Returns `None`
        when no frame has been decoded yet (camera just started, or
        connection is broken). Encode happens under the stats lock so
        the frame ndarray doesn't get reassigned mid-encode.
        """
        with self._stats_lock:
            frame = self._latest_frame
            if frame is None:
                return None
            # cv2.imencode releases the GIL during the encode step and
            # treats `frame` as read-only, so it's safe to hold the
            # lock across the call. Still, profile if camera count grows.
            import cv2  # local import: avoid pulling cv2 into modules that don't decode

            ok, buf = cv2.imencode(".jpg", frame, [cv2.IMWRITE_JPEG_QUALITY, quality])
            if not ok:
                return None
            return buf.tobytes()

    # ----- internals -----

    def _run(self) -> None:
        # Consent gates ANALYTICS (zones, lines, VLM, event emission), not
        # frame decoding. Even with consent off, we keep decoding so the
        # preview server can serve thumbnails — the operator needs to be
        # able to SEE the camera before deciding whether to opt in.
        # `_frame_loop` reads `self.cfg.analytics_consent` on every frame,
        # so flipping the flag in Oxy + restarting the worker (or, later,
        # via the hot-reload TODO) takes effect without any code path
        # split here.
        if not self.cfg.analytics_consent:
            log("info", "camera.consent_off",
                camera=self.cfg.name,
                note="decoding for preview only; no analytics or events")
        if self.cfg.analytics_consent and not self._zones and not self._lines:
            log("info", "camera.whole_frame_fallback", camera=self.cfg.name,
                note="no zones/lines configured; treating whole frame as one implicit zone")

        log(
            "info", "camera.config",
            camera=self.cfg.name, tracker=TRACKER,
            trigger_dwell=TRIGGER_DWELL_SEC, track_cooldown=TRACK_COOLDOWN_SEC,
            global_min=GLOBAL_MIN_INTERVAL_SEC, vlm_model=VLM_MODEL,
            vlm_enabled=self.vlm_client is not None,
            analytics_consent=self.cfg.analytics_consent,
            zones=list(self._zones), lines=list(self._lines),
        )

        backoff = 1.0
        while not self._stop.is_set():
            cap = cv2.VideoCapture(self.cfg.rtsp_url, cv2.CAP_FFMPEG)
            if not cap.isOpened():
                with self._stats_lock:
                    self._reconnects += 1
                log("warn", "camera.open_failed", camera=self.cfg.name,
                    rtsp=self.cfg.rtsp_url, backoff_s=backoff)
                time.sleep(backoff)
                backoff = min(backoff * 2, 30.0)
                continue

            log("info", "camera.connected", camera=self.cfg.name)
            backoff = 1.0
            try:
                self._frame_loop(cap)
            finally:
                cap.release()

    def _frame_loop(self, cap: "cv2.VideoCapture") -> None:
        while not self._stop.is_set():
            ok, frame = cap.read()
            if not ok:
                with self._stats_lock:
                    self._decoder_errors += 1
                log("warn", "camera.read_failed", camera=self.cfg.name)
                return  # reconnect from outer loop

            now = datetime.now(timezone.utc)
            with self._stats_lock:
                self._frames += 1
                self._last_frame_at = now
                self._latest_frame = frame

            # Frame is captured + available for preview. Everything
            # below this point is analytics: YOLO inference, zone /
            # line evaluation, event emission. Skip when consent is
            # off — the operator gets a live thumbnail without us
            # generating any compliance signal until they opt in.
            if not self.cfg.analytics_consent:
                continue

            try:
                detections = self._detect_and_track(frame)
            except Exception as exc:  # noqa: BLE001 - we never want to die mid-stream
                log("error", "camera.detect_error", camera=self.cfg.name, error=str(exc))
                continue

            # Dispatch by configuration:
            #   - Zones or lines configured → use them (operator
            #     drew explicit polygons / counters).
            #   - Neither configured → treat the whole frame as one
            #     implicit zone. This is the new-camera default and
            #     also the multi-camera-per-restaurant fallback
            #     until per-camera role tagging is added.
            if self._zones or self._lines:
                self._handle_zones(detections, now, frame)
                self._handle_lines(detections, now)
            else:
                self._handle_whole_frame(detections, now, frame)

    def _detect_and_track(self, frame: np.ndarray) -> sv.Detections:
        results = self._model.track(
            frame,
            persist=True,
            tracker=TRACKER,
            classes=[PERSON_CLASS_ID],
            verbose=False,
        )[0]
        detections = sv.Detections.from_ultralytics(results)
        return self._smoother.update_with_detections(detections)

    # ----- zone state + on-trigger VLM -----

    def _handle_zones(self, detections: sv.Detections, ts: datetime, frame: np.ndarray) -> None:
        tids = detections.tracker_id.tolist() if detections.tracker_id is not None else []
        for zone_id, zone in self._zones.items():
            mask = zone.trigger(detections=detections)
            present_now = {tids[i] for i in range(len(tids)) if mask[i]}
            present_prev = self._in_zone_prev[zone_id]

            for tid in present_now - present_prev:
                self._emit_event("enter", ts, track_id=str(tid), zone_id=zone_id)
                self._track_state[(zone_id, tid)].in_zone_since = ts

            for tid in present_prev - present_now:
                state = self._track_state[(zone_id, tid)]
                dwell = (ts - state.in_zone_since).total_seconds() if state.in_zone_since else None
                self._emit_event("exit", ts, track_id=str(tid), zone_id=zone_id, dwell_seconds=dwell)
                state.in_zone_since = None
                state.last_dwell_emit = None

            self._in_zone_prev[zone_id] = present_now

            # Sustained-occupancy (congestion) check on the zone head count.
            # `len(present_now)` is the count of tracked ids in the zone this
            # frame — tracker-derived, but a per-frame count is churn-robust (we
            # don't rely on id continuity) and needs no appearance/identity.
            flag = self._congestion.update(zone_id, len(present_now), ts)
            if flag is not None:
                self._trigger_congestion(flag, ts)

            # Dwell tick + on-trigger VLM check.
            for tid in present_now:
                state = self._track_state[(zone_id, tid)]
                if state.in_zone_since is None:
                    continue
                dwell = (ts - state.in_zone_since).total_seconds()

                if (
                    state.last_dwell_emit is None
                    or (ts - state.last_dwell_emit).total_seconds() >= DWELL_EVENT_EVERY_SEC
                ):
                    self._emit_event("dwell", ts, track_id=str(tid), zone_id=zone_id, dwell_seconds=dwell)
                    state.last_dwell_emit = ts

                self._maybe_trigger_vlm(zone_id, tid, state, dwell, ts, frame)

    def _handle_whole_frame(
        self, detections: sv.Detections, ts: datetime, frame: np.ndarray
    ) -> None:
        """Fallback trigger path when no zones/lines are configured.

        Treats the whole frame as one implicit zone — same enter /
        exit / dwell semantics as `_handle_zones`, same per-track +
        global VLM cooldowns. The only difference is the zone test
        is trivially "true for every tracked person."

        Events emitted carry `zone_id=WHOLE_FRAME_ZONE_ID` so the
        Airhouse-side analytics can tell synthetic zones apart from
        operator-drawn ones (e.g. for "what fraction of compliance
        signal comes from cameras that haven't been zoned yet").

        Track ids come from the tracker and persist as long as the person
        stays in view. Walking out and back in mints a fresh id, so
        a returning person gets re-checked — that's intentional, the
        per-track cooldown is per-id, not per-human.
        """
        tids = detections.tracker_id.tolist() if detections.tracker_id is not None else []
        present_now = {int(t) for t in tids}
        present_prev = self._in_zone_prev[WHOLE_FRAME_ZONE_ID]

        for tid in present_now - present_prev:
            self._emit_event(
                "enter", ts, track_id=str(tid), zone_id=WHOLE_FRAME_ZONE_ID
            )
            self._track_state[(WHOLE_FRAME_ZONE_ID, tid)].in_zone_since = ts

        for tid in present_prev - present_now:
            state = self._track_state[(WHOLE_FRAME_ZONE_ID, tid)]
            dwell = (
                (ts - state.in_zone_since).total_seconds() if state.in_zone_since else None
            )
            self._emit_event(
                "exit",
                ts,
                track_id=str(tid),
                zone_id=WHOLE_FRAME_ZONE_ID,
                dwell_seconds=dwell,
            )
            state.in_zone_since = None
            state.last_dwell_emit = None

        self._in_zone_prev[WHOLE_FRAME_ZONE_ID] = present_now

        # Dwell tick + VLM trigger — same shape as the zone path.
        for tid in present_now:
            state = self._track_state[(WHOLE_FRAME_ZONE_ID, tid)]
            if state.in_zone_since is None:
                continue
            dwell = (ts - state.in_zone_since).total_seconds()

            if (
                state.last_dwell_emit is None
                or (ts - state.last_dwell_emit).total_seconds() >= DWELL_EVENT_EVERY_SEC
            ):
                self._emit_event(
                    "dwell",
                    ts,
                    track_id=str(tid),
                    zone_id=WHOLE_FRAME_ZONE_ID,
                    dwell_seconds=dwell,
                )
                state.last_dwell_emit = ts

            self._maybe_trigger_vlm(
                WHOLE_FRAME_ZONE_ID, tid, state, dwell, ts, frame
            )

    # ----- congestion (sustained occupancy) -----

    def _trigger_congestion(self, flag: CongestionFlag, ts: datetime) -> None:
        """Worker-thread side: a zone just flagged a sustained backup. Hand off
        to the main loop to pull + archive the clip and emit the event — same
        shape as the VLM trigger, so the frame thread never blocks on network
        I/O."""
        segment_start = ts - timedelta(seconds=CONGESTION_CLIP_SEC)
        log("info", "camera.congestion", camera=self.cfg.name, zone=flag.zone_id,
            count=flag.count, sustained_s=round(flag.sustained_sec, 1))
        asyncio.run_coroutine_threadsafe(
            self._archive_and_emit_congestion(flag, segment_start, ts),
            self.loop,
        )

    async def _archive_and_emit_congestion(
        self, flag: CongestionFlag, segment_start: datetime, ts: datetime
    ) -> None:
        """Main-loop side: archive the congestion window (best-effort) and emit
        a `congestion` event carrying the clip key. Never raises into the loop —
        the event ships even if archival is off or fails."""
        # One id for both the event and the clip key stem, so the archived clip
        # (`{prefix}/{wid}/{date}/{event_id}.mp4`) is trivially correlated back
        # to its congestion event.
        event_id = str(uuid4())
        key: str | None = None
        if self._oxy_client is not None:
            # Zone polygon (live-stream pixel space) + the live frame's
            # dims → the archiver bakes the zone/boxes/count overlay into
            # the clip so the evidence shows the CV that fired the flag.
            zone = self._zones.get(flag.zone_id)
            polygon = zone.polygon if zone is not None else None
            with self._stats_lock:
                fr = self._latest_frame
            source_wh = (fr.shape[1], fr.shape[0]) if fr is not None else None
            try:
                key = await upload_window_clip(
                    oxy_client=self._oxy_client,
                    report_id=event_id,
                    camera_id=str(self.cfg.id),
                    segment_start=segment_start,
                    segment_end=ts,
                    annotate_zone=polygon,
                    source_wh=source_wh,
                )
            except Exception as exc:  # noqa: BLE001 — log + still emit the event
                log("warn", "camera.congestion_archive_failed",
                    camera=self.cfg.name, zone=flag.zone_id, error=str(exc))
        # Zone-level event: no single track (track_id empty). The head count
        # rides in `confidence` (the slot line_cross already reuses) and the
        # sustained length in `dwell_seconds`.
        event = self._event_dict(
            "congestion", ts, event_id=event_id, track_id="", zone_id=flag.zone_id,
            dwell_seconds=flag.sustained_sec, confidence=float(flag.count),
            evidence_s3_key=key,
        )
        await self.queue.put({"kind": "event", "payload": event})
        log("info", "camera.congestion_emitted", camera=self.cfg.name,
            zone=flag.zone_id, archived=key is not None)

    def _maybe_trigger_vlm(
        self,
        zone_id: str,
        tid: int,
        state: TrackState,
        dwell: float,
        ts: datetime,
        frame: np.ndarray,
    ) -> None:
        if self.vlm_client is None:
            return
        if dwell < TRIGGER_DWELL_SEC:
            return
        if (
            state.last_vlm_check is not None
            and (ts - state.last_vlm_check).total_seconds() < TRACK_COOLDOWN_SEC
        ):
            return
        if (
            self._last_global_vlm is not None
            and (ts - self._last_global_vlm).total_seconds() < GLOBAL_MIN_INTERVAL_SEC
        ):
            return

        # Capture frame as JPEG bytes BEFORE handing off; the frame buffer the
        # thread is reusing must not race with the coroutine that encodes it.
        ok, buf = cv2.imencode(".jpg", frame, [cv2.IMWRITE_JPEG_QUALITY, 85])
        if not ok:
            log("warn", "camera.jpeg_encode_failed", camera=self.cfg.name)
            return

        segment_start = state.in_zone_since or ts
        # P3 (Option C) — PPE-YOLO bbox detections on the same frame we
        # send to the VLM. Run BEFORE the asyncio handoff so we're still
        # on the same buffer (the encode above doesn't mutate, but the
        # frame thread is free to overwrite once we return). `infer()`
        # is a no-op + returns None when PPE_YOLO_MODEL is unset, so this
        # is free when the operator hasn't enabled the model. The envelope
        # rides through the coroutine and onto the compliance report
        # payload's `detections_json` field; see service::compliance.
        frame_offset_ms = int((ts - segment_start).total_seconds() * 1000)
        detections_envelope = ppe_yolo.infer(frame, frame_offset_ms)

        state.last_vlm_check = ts
        self._last_global_vlm = ts

        log("info", "camera.vlm_trigger", camera=self.cfg.name,
            zone=zone_id, track=tid, dwell_s=round(dwell, 1),
            ppe_detections=(len(detections_envelope.get("detections", []))
                            if detections_envelope else None))

        # Hand off to the main asyncio loop.
        asyncio.run_coroutine_threadsafe(
            self._vlm_check(
                track_id=tid,
                dwell_seconds=dwell,
                jpeg_bytes=bytes(buf),
                segment_start=segment_start,
                segment_end=ts,
                detections_envelope=detections_envelope,
            ),
            self.loop,
        )

    async def _vlm_check(
        self,
        track_id: int,
        dwell_seconds: float,
        jpeg_bytes: bytes,
        segment_start: datetime,
        segment_end: datetime,
        detections_envelope: dict[str, Any] | None = None,
    ) -> None:
        """Runs on the main asyncio loop. Calls Claude async, enqueues the
        result to the outbox. Never raises into the loop — we log and drop."""
        if self.vlm_client is None:
            return
        # `render_prompt` returns the active pack's role template
        # (Jinja2-rendered with `variables`, `track_id`, `dwell_seconds`)
        # or None if no pack is installed / the template failed to
        # render. None means skip this check — the next config poll
        # may pick up a valid pack. Skipping is safer than guessing
        # a fallback prompt; a misconfigured pack should not silently
        # produce compliance reports against the wrong protocol.
        prompt = render_prompt(
            self.cfg.role, track_id=track_id, dwell_seconds=dwell_seconds
        )
        if prompt is None:
            log("warn", "camera.vlm_skipped_no_prompt",
                camera=self.cfg.name, track=track_id, role=self.cfg.role)
            return
        img_b64 = base64.b64encode(jpeg_bytes).decode()
        try:
            resp = await self.vlm_client.messages.create(
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
        except anthropic.APIError as exc:
            log("warn", "camera.vlm_api_error", camera=self.cfg.name,
                track=track_id, error=str(exc))
            return

        raw = resp.content[0].text if resp.content else ""
        tokens = (resp.usage.input_tokens or 0) + (resp.usage.output_tokens or 0)
        parsed: dict[str, Any] | None = None
        match = re.search(r"\{.*\}", raw, re.DOTALL)
        if match:
            try:
                parsed = json.loads(match.group())
            except json.JSONDecodeError:
                parsed = None

        report_id = str(uuid4())
        report = {
            "report_id": report_id,
            "camera_id": str(self.cfg.id),
            "segment_start": segment_start.isoformat(),
            "segment_end": segment_end.isoformat(),
            "trigger_type": "sustained_presence",
            "trigger_track_id": str(track_id),
            "vlm_model": VLM_MODEL,
            "report_text": raw,
            "structured_json": parsed or {},
            "frame_uri": None,
            "tokens_used": tokens,
            # Tier B #6 — stays None unless the post-VLM
            # archival step below succeeds. Set BEFORE queue.put
            # so the outbox sees the right shape on the first
            # attempt; we never re-edit a queued payload.
            "evidence_s3_key": None,
            # P3 (Option C) — PPE-YOLO bbox envelope computed on the
            # frame thread before this coroutine was scheduled. None
            # when PPE_YOLO_MODEL is unset OR inference failed; the
            # backend treats `None` as "edge worker doesn't do PPE
            # yet" and an empty inner detections array as "ran, saw
            # nothing." Contract documented on the Rust side at
            # crates/cameras/src/service/compliance.rs::CompliancePayload.
            "detections_json": detections_envelope,
        }

        # Tier B #6 — archive the dwell window when the VLM
        # flagged a compliance violation. `_is_violation` looks
        # at attire/presence flags AND confidence threshold so
        # empty-frame false-positives don't pay for an upload.
        # The server's `/control/clips/sign` route returns 503
        # when S3 isn't configured, so deployments without an
        # archive bucket trip the no-op branch quietly.
        # Failures inside `upload_window_clip` log + return
        # None — the compliance report ships either way.
        if self._oxy_client is not None and _is_violation(parsed):
            key = await upload_window_clip(
                oxy_client=self._oxy_client,
                report_id=report_id,
                camera_id=str(self.cfg.id),
                segment_start=segment_start,
                segment_end=segment_end,
            )
            if key:
                report["evidence_s3_key"] = key

        # Already on the main loop — put directly.
        await self.queue.put({"kind": "compliance_report", "payload": report})
        log("info", "camera.vlm_done", camera=self.cfg.name,
            track=track_id, tokens=tokens, parsed=parsed is not None,
            archived=report["evidence_s3_key"] is not None)

    # ----- lines -----

    def _handle_lines(self, detections: sv.Detections, ts: datetime) -> None:
        tids = detections.tracker_id.tolist() if detections.tracker_id is not None else []
        for line_id, line in self._lines.items():
            crossed_in, crossed_out = line.trigger(detections)
            for i, (ci, co) in enumerate(zip(crossed_in, crossed_out)):
                if not (ci or co) or i >= len(tids):
                    continue
                self._emit_event(
                    "line_cross", ts, track_id=str(tids[i]), line_id=line_id,
                    # +/- direction encoded in confidence for now
                    confidence=1.0 if ci else 0.0,
                )

    # ----- emit -----

    def _event_dict(
        self,
        kind: str,
        ts: datetime,
        *,
        track_id: str,
        event_id: str | None = None,
        zone_id: str | None = None,
        line_id: str | None = None,
        dwell_seconds: float | None = None,
        confidence: float | None = None,
        evidence_s3_key: str | None = None,
    ) -> dict[str, Any]:
        event = {
            "event_id": event_id or str(uuid4()),
            "ts": ts.isoformat(),
            "camera_id": str(self.cfg.id),
            "event_type": kind,
            "zone_id": zone_id,
            "line_id": line_id,
            "track_id": track_id,
            "dwell_seconds": dwell_seconds,
            "confidence": confidence,
            "frame_uri": None,
        }
        # Only present on congestion events. The server ignores unknown fields
        # until the oxy_cam_events `evidence_s3_key` column lands (Phase 2), so
        # this is forward-compatible and a no-op for the other event kinds.
        if evidence_s3_key is not None:
            event["evidence_s3_key"] = evidence_s3_key
        return event

    def _emit_event(
        self,
        kind: str,
        ts: datetime,
        *,
        track_id: str,
        zone_id: str | None = None,
        line_id: str | None = None,
        dwell_seconds: float | None = None,
        confidence: float | None = None,
    ) -> None:
        event = self._event_dict(
            kind,
            ts,
            track_id=track_id,
            zone_id=zone_id,
            line_id=line_id,
            dwell_seconds=dwell_seconds,
            confidence=confidence,
        )
        # Thread-safe handoff. The outbox-producer task on the main loop pulls
        # from this queue and routes by `kind`.
        asyncio.run_coroutine_threadsafe(
            self.queue.put({"kind": "event", "payload": event}),
            self.loop,
        )
