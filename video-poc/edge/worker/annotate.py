"""Bake the congestion overlay into an evidence clip.

Takes the raw fMP4 the worker just pulled from MediaMTX and returns an
H.264 mp4 with the `service_area` zone outlined, a box drawn on every
tracked person (green inside the zone, dim outside), and a live
"N in zone" count burned into each frame — so the archived clip
visibly shows the CV that actually fired the congestion flag, not a
raw camera dump.

Why re-run detection here instead of reusing the live loop: the clip
covers the ~30s *before* the flag, and those frames already streamed
past `CameraReader._frame_loop`. We can't recover the live detections
for them, so we decode the clip and detect again — with a DEDICATED
model (its own instance, so clip inference never perturbs a live tracker).

Detection here is plain per-frame `.predict()`, NOT `.track()`: a tracker
fed subsampled clip frames keeps *lost* tracks alive and drifts their
predicted boxes across empty frames (bytetrack's ~30-frame buffer), which
showed up as boxes jumping around with no one present. Predict draws a box
only where a person is actually detected on that frame (see `_detect`).

Why ffmpeg for the encode: `opencv-python-headless` bundles an ffmpeg
without libx264, so `cv2.VideoWriter` can't emit browser-playable
H.264. The system `ffmpeg` (apt-installed in the image) has libx264,
so we decode with cv2, draw with cv2, and pipe raw frames to ffmpeg.

Everything here is best-effort: any failure returns None and the
caller uploads the raw clip unchanged — an un-annotated clip beats a
lost clip.
"""
from __future__ import annotations

import os
import subprocess
import tempfile
import threading

import cv2  # type: ignore[import-untyped]
import numpy as np
import supervision as sv  # type: ignore[import-untyped]

from .congestion import CONGESTION_MIN_COUNT
from .log import log

# Same weights + tracker + person class as the live loop (camera.py). A
# separate model instance from the per-camera models so clip inference
# never perturbs a live tracker; we reset ITS tracker per clip below.
_YOLO_MODEL = os.environ.get("YOLO_MODEL", "yolo11n.pt")
_PERSON_CLASS_ID = 0  # COCO person
# Confidence gate for the boxes we draw. A touch above the model default (0.25)
# to gate the weakest false positives without dropping real (often far/low-conf,
# ~0.3-0.45) CCTV people; the real phantom fix is dropping the tracker. Tunable.
_CONF = float(os.environ.get("CLIP_ANNOTATE_CONF", "0.3"))

# Master toggle — set CLIP_ANNOTATE_ENABLED=0 to ship raw clips.
_ENABLED = os.environ.get("CLIP_ANNOTATE_ENABLED", "1") != "0"
# Re-running YOLO on a CPU box is the cost here, so we detect on a
# subsampled cadence and carry the last boxes forward on the frames in
# between (the output stays full-fps and smooth; boxes refresh ~6x/s).
_DETECT_FPS = float(os.environ.get("CLIP_ANNOTATE_DETECT_FPS", "6"))
# Hard ceiling on frames processed — a runaway/garbled fMP4 can report a
# bogus length; this bounds the worst-case CPU spend regardless. Sized to
# cover a 15-min evidence clip (CONGESTION_CLIP_SEC=900) at ~30fps: it MUST
# exceed CONGESTION_CLIP_SEC × fps or the annotated clip is silently
# truncated to this many frames. NB: annotating a 15-min clip is minutes of
# CPU on the t4g — if the box falls behind, ship long clips raw
# (CLIP_ANNOTATE_ENABLED=0) or drop the detect cadence (CLIP_ANNOTATE_DETECT_FPS).
_MAX_FRAMES = int(os.environ.get("CLIP_ANNOTATE_MAX_FRAMES", "28000"))
# libx264 speed/size trade-off; veryfast keeps a 30s clip to seconds on
# the t4g.medium edge box.
_X264_PRESET = os.environ.get("CLIP_ANNOTATE_X264_PRESET", "veryfast")

# Colors are BGR (cv2 convention).
_ZONE_BGR = (0, 200, 255)       # amber zone outline + fill
_IN_ZONE_BGR = (70, 210, 70)    # green box for people inside the zone
_OUT_ZONE_BGR = (150, 150, 150) # dim gray box for people outside
_OK_BGR = (70, 210, 70)         # count below threshold
_ALERT_BGR = (60, 60, 235)      # count at/above threshold (congested)

# One shared dedicated model, loaded lazily and reused across clips.
# `model.predict` is not safe to call concurrently on one instance, and
# congestion clips are rare, so we serialize annotation under this lock.
_model = None
_model_lock = threading.Lock()


def _load_model():
    global _model
    if _model is None:
        from ultralytics import YOLO  # type: ignore[import-untyped]

        _model = YOLO(_YOLO_MODEL)
    return _model


def annotate_clip_bytes(
    raw: bytes,
    *,
    polygon: "np.ndarray | list | None",
    source_wh: "tuple[int, int] | None" = None,
) -> bytes | None:
    """Return an H.264 mp4 with the zone + person boxes + live count
    drawn in, or None if annotation is disabled/unavailable/failed
    (caller falls back to the raw bytes).

    `polygon` is the zone in the LIVE stream's pixel space; `source_wh`
    is that stream's (w, h). If the clip decodes at a different
    resolution (main vs sub stream), we rescale the polygon to match.
    Runs synchronously and CPU-bound — call it via `asyncio.to_thread`.
    """
    if not _ENABLED or polygon is None:
        return None
    poly = np.asarray(polygon, dtype=np.float64)
    if poly.ndim != 2 or poly.shape[0] < 3:
        return None
    with _model_lock:
        try:
            return _annotate_locked(raw, poly, source_wh)
        except Exception as e:  # noqa: BLE001 — never fail the upload
            log("warn", "annotate.failed", error=str(e))
            return None


def _annotate_locked(
    raw: bytes, poly: np.ndarray, source_wh: "tuple[int, int] | None"
) -> bytes | None:
    in_path = out_path = None
    cap = None
    try:
        in_path = _write_temp(raw, suffix=".mp4")
        out_path = _temp_path(suffix=".mp4")
        cap = cv2.VideoCapture(in_path)
        if not cap.isOpened():
            log("warn", "annotate.decode_open_failed")
            return None
        w = int(cap.get(cv2.CAP_PROP_FRAME_WIDTH)) or (source_wh[0] if source_wh else 0)
        h = int(cap.get(cv2.CAP_PROP_FRAME_HEIGHT)) or (source_wh[1] if source_wh else 0)
        fps = cap.get(cv2.CAP_PROP_FPS)
        if not (w and h):
            log("warn", "annotate.no_frame_dims")
            return None
        fps = fps if fps and fps > 1 else 15.0

        # Count by box CENTER, not supervision's default BOTTOM_CENTER (feet).
        # In these counter scenes people stand behind the prep line with their
        # feet occluded, so a feet-anchor lands at/below the zone edge and they
        # read as out-of-zone (grey box, uncounted) even though they're plainly
        # inside it. Center is robust to occluded feet. Kept in sync with
        # camera.py's zone build so the annotation matches the real count.
        zone = sv.PolygonZone(
            polygon=_scale_polygon(poly, source_wh, (w, h)),
            triggering_anchors=(sv.Position.CENTER,),
        )
        n_written = _render(cap, zone, (w, h), fps, out_path)
        if n_written == 0:
            log("warn", "annotate.no_frames_rendered")
            return None
        with open(out_path, "rb") as f:
            data = f.read()
        return data or None
    finally:
        if cap is not None:
            cap.release()
        _cleanup(in_path, out_path)


def _render(cap, zone, wh: "tuple[int, int]", fps: float, out_path: str) -> int:
    """Decode → detect (subsampled) → draw → pipe to ffmpeg. Returns the
    number of frames written."""
    w, h = wh
    detect_every = max(1, int(round(fps / _DETECT_FPS)) if _DETECT_FPS > 0 else 1)
    proc = _open_encoder(out_path, w, h, fps)
    det = None  # last detections, carried forward between detects
    mask = None
    count = 0
    idx = 0
    written = 0
    try:
        while idx < _MAX_FRAMES:
            ok, frame = cap.read()
            if not ok:
                break
            if frame.shape[1] != w or frame.shape[0] != h:
                frame = cv2.resize(frame, (w, h))
            if idx % detect_every == 0:
                det = _detect(frame)
                mask = zone.trigger(detections=det) if len(det) else np.array([], bool)
                count = int(mask.sum()) if mask is not None else 0
            _draw_frame(frame, zone, det, mask, count)
            proc.stdin.write(frame.tobytes())
            written += 1
            idx += 1
    finally:
        _close_encoder(proc)
    return written


def _detect(frame: np.ndarray) -> sv.Detections:
    """Per-frame person detection for annotation — plain `.predict()`, NOT
    `.track()`. A tracker fed *subsampled* clip frames keeps lost tracks alive
    (bytetrack's ~30-frame buffer) and drifts their Kalman-predicted boxes
    across frames where no one is present — that is the "boxes jump around with
    no people there" bug. `.predict()` puts a box only where YOLO actually sees
    a person on this frame; `_CONF` gates low-confidence phantoms off clutter.
    (We lose stable per-track ids — fine for evidence; the count banner is the
    number that matters.)"""
    results = _load_model().predict(
        frame,
        conf=_CONF,
        classes=[_PERSON_CLASS_ID],
        verbose=False,
    )[0]
    return sv.Detections.from_ultralytics(results)


def _draw_frame(frame, zone, det, mask, count: int) -> None:
    """Draw zone fill/outline, per-person boxes, and the count banner."""
    _draw_zone(frame, zone.polygon)
    if det is not None and len(det):
        for i in range(len(det)):
            inside = bool(mask[i]) if mask is not None and i < len(mask) else False
            tid = int(det.tracker_id[i]) if det.tracker_id is not None else None
            _draw_box(frame, det.xyxy[i], inside, tid)
    _draw_banner(frame, count)


def _draw_zone(frame, polygon: np.ndarray) -> None:
    pts = np.asarray(polygon, dtype=np.int32).reshape(-1, 1, 2)
    overlay = frame.copy()
    cv2.fillPoly(overlay, [pts], _ZONE_BGR)
    cv2.addWeighted(overlay, 0.15, frame, 0.85, 0, dst=frame)
    cv2.polylines(frame, [pts], isClosed=True, color=_ZONE_BGR, thickness=2, lineType=cv2.LINE_AA)


def _draw_box(frame, xyxy, inside: bool, tid: "int | None") -> None:
    x1, y1, x2, y2 = (int(v) for v in xyxy)
    color = _IN_ZONE_BGR if inside else _OUT_ZONE_BGR
    cv2.rectangle(frame, (x1, y1), (x2, y2), color, 2 if inside else 1, cv2.LINE_AA)
    if inside and tid is not None:
        label = f"#{tid}"
        (tw, th), _ = cv2.getTextSize(label, cv2.FONT_HERSHEY_SIMPLEX, 0.5, 1)
        cv2.rectangle(frame, (x1, y1 - th - 6), (x1 + tw + 6, y1), color, -1)
        cv2.putText(frame, label, (x1 + 3, y1 - 4),
                    cv2.FONT_HERSHEY_SIMPLEX, 0.5, (20, 20, 20), 1, cv2.LINE_AA)


def _draw_banner(frame, count: int) -> None:
    """Top-left 'service area: N in zone' badge; the count turns red once
    it reaches the congestion threshold."""
    h = frame.shape[0]
    scale = max(0.6, h / 1080.0)
    num_color = _ALERT_BGR if count >= CONGESTION_MIN_COUNT else _OK_BGR
    pad = int(12 * scale)
    line1 = "SERVICE AREA"
    line2 = f"{count} in zone"
    (w1, h1), _ = cv2.getTextSize(line1, cv2.FONT_HERSHEY_SIMPLEX, 0.6 * scale, 1)
    (w2, h2), _ = cv2.getTextSize(line2, cv2.FONT_HERSHEY_DUPLEX, 1.1 * scale, 2)
    box_w = max(w1, w2) + 2 * pad
    box_h = h1 + h2 + 3 * pad
    overlay = frame.copy()
    cv2.rectangle(overlay, (0, 0), (box_w, box_h), (25, 25, 25), -1)
    cv2.addWeighted(overlay, 0.55, frame, 0.45, 0, dst=frame)
    cv2.putText(frame, line1, (pad, pad + h1),
                cv2.FONT_HERSHEY_SIMPLEX, 0.6 * scale, (210, 210, 210), 1, cv2.LINE_AA)
    cv2.putText(frame, line2, (pad, box_h - pad),
                cv2.FONT_HERSHEY_DUPLEX, 1.1 * scale, num_color, 2, cv2.LINE_AA)


def _open_encoder(out_path: str, w: int, h: int, fps: float) -> "subprocess.Popen":
    """ffmpeg reading raw BGR frames on stdin, writing faststart H.264."""
    cmd = [
        "ffmpeg", "-hide_banner", "-loglevel", "error", "-y",
        "-f", "rawvideo", "-pix_fmt", "bgr24", "-s", f"{w}x{h}",
        "-r", f"{fps:.3f}", "-i", "-",
        "-an", "-c:v", "libx264", "-preset", _X264_PRESET,
        "-pix_fmt", "yuv420p", "-movflags", "+faststart", out_path,
    ]
    return subprocess.Popen(cmd, stdin=subprocess.PIPE, stdout=subprocess.DEVNULL,
                            stderr=subprocess.PIPE)


def _close_encoder(proc: "subprocess.Popen") -> None:
    # We wrote frames to stdin ourselves, so signal EOF by closing it,
    # then drain stderr (near-empty at -loglevel error) and wait. We
    # avoid Popen.communicate() here: it re-flushes the already-closed
    # stdin and raises "flush of closed file".
    try:
        if proc.stdin:
            proc.stdin.close()
    except Exception:  # noqa: BLE001
        pass
    err = b""
    try:
        if proc.stderr:
            err = proc.stderr.read() or b""
    except Exception:  # noqa: BLE001
        pass
    try:
        proc.wait(timeout=120)
    except Exception:  # noqa: BLE001
        proc.kill()
    if proc.returncode not in (0, None):
        log("warn", "annotate.ffmpeg_nonzero", rc=proc.returncode,
            err=err.decode("utf-8", "replace")[-300:])


def _scale_polygon(poly: np.ndarray, src_wh, dst_wh) -> np.ndarray:
    """Rescale zone points from the authoring (live-stream) resolution to
    the clip's decode resolution when they differ."""
    if not src_wh:
        return poly.astype(np.int32)
    sw, sh = src_wh
    dw, dh = dst_wh
    if not (sw and sh) or (sw == dw and sh == dh):
        return poly.astype(np.int32)
    scaled = poly.astype(np.float64).copy()
    scaled[:, 0] *= dw / sw
    scaled[:, 1] *= dh / sh
    return scaled.astype(np.int32)


def _write_temp(data: bytes, *, suffix: str) -> str:
    fd, path = tempfile.mkstemp(suffix=suffix)
    with os.fdopen(fd, "wb") as f:
        f.write(data)
    return path


def _temp_path(*, suffix: str) -> str:
    fd, path = tempfile.mkstemp(suffix=suffix)
    os.close(fd)
    return path


def _cleanup(*paths: "str | None") -> None:
    for p in paths:
        if p:
            try:
                os.remove(p)
            except OSError:
                pass
