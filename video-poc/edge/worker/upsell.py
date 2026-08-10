"""Edge audio upsell-detection reader (Phase 2).

Per register camera: pull the employee-mic audio off the camera RTSP,
transcribe on-box (faster-whisper), classify upsell intent (Haiku), and
emit `upsell_attempt` camera-events through the same outbox queue the video
readers use. Transcribe-and-discard — no audio is persisted.

Opt-in per box via the `UPSELL_CAMERAS` env (comma-separated camera-name
substrings) — the tuned near-field mic is a physical property of specific
register cameras, so the operator that tuned it names them, and no backend
config change is needed. One AudioReader thread runs per matching camera,
spawned alongside its CameraReader in `main.run()`; the STT model is loaded
once and shared across readers.

Pipeline (mirrors `video-poc/audio-poc/pipeline.py`):
    stream_pcm → window buffer → transcribe → intent.classify
                                            → dedup (per-item cooldown)
                                            → emit `upsell_attempt`
"""
from __future__ import annotations

import asyncio
import os
import threading
from datetime import datetime, timezone
from typing import Any
from uuid import uuid4

import numpy as np

from . import upsell_capture
from .config import CameraConfig
from .log import log
from .upsell_intent import Verdict, classify
from .upsell_stt import Transcriber

WINDOW_SEC = float(os.environ.get("UPSELL_WINDOW_SEC", "12.0"))
CONF_THRESHOLD = float(os.environ.get("UPSELL_CONF_THRESHOLD", "0.5"))
# One offer can straddle two transcription windows; collapse the duplicate
# by not re-emitting the same item within this window.
ITEM_COOLDOWN_SEC = float(os.environ.get("UPSELL_ITEM_COOLDOWN_SEC", "30.0"))


def upsell_cameras() -> list[str]:
    """Parse UPSELL_CAMERAS → lowercased name-substrings. Empty ⇒ disabled."""
    raw = os.environ.get("UPSELL_CAMERAS", "").strip()
    return [s.strip().lower() for s in raw.split(",") if s.strip()]


def camera_enabled(name: str, allow: list[str]) -> bool:
    """A camera runs audio upsell if its name contains any allowlist entry
    (substring, case-insensitive) — so `register` matches
    `PH - Santa Clara - Register`."""
    n = (name or "").lower()
    return any(sub in n for sub in allow)


def should_emit(
    v: Verdict,
    window_end: float,
    last_emit: dict[str, float],
    *,
    threshold: float = CONF_THRESHOLD,
    cooldown: float = ITEM_COOLDOWN_SEC,
) -> bool:
    """Pure emit decision. Returns True (and records the emit time) when the
    verdict is a confident upsell for an item not seen within `cooldown`."""
    if not (v.is_upsell and v.confidence >= threshold):
        return False
    key = (v.item or "unknown").lower()
    if window_end - last_emit.get(key, -1e9) < cooldown:
        return False
    last_emit[key] = window_end
    return True


def upsell_event(camera_id: str, v: Verdict) -> dict[str, Any]:
    """The `upsell_attempt` camera-event. Same shape as camera.py's
    `_event_dict`; `label` carries the offered item (the oxy_cam_events
    `label` column), `confidence` the classifier score."""
    return {
        "event_id": str(uuid4()),
        "ts": datetime.now(timezone.utc).isoformat(),
        "camera_id": camera_id,
        "event_type": "upsell_attempt",
        "zone_id": None,
        "line_id": None,
        "track_id": "",
        "dwell_seconds": None,
        "confidence": round(v.confidence, 3),
        "frame_uri": None,
        "label": v.item,
    }


class AudioReader:
    """One audio-upsell thread for a single register camera."""

    def __init__(
        self,
        cfg: CameraConfig,
        loop: asyncio.AbstractEventLoop,
        queue: asyncio.Queue,
        stt: Transcriber,
        intent_client: Any,
        *,
        enhance: bool = True,
    ) -> None:
        self.cfg = cfg
        self.loop = loop
        self.queue = queue
        self._stt = stt
        self._intent_client = intent_client
        self._enhance = enhance
        self._stop = threading.Event()
        self._stats_lock = threading.Lock()
        self._windows = 0
        self._attempts = 0
        self._errors = 0
        self._last_text_at: datetime | None = None

    # ----- public api (mirrors CameraReader) -----

    def start(self) -> threading.Thread:
        t = threading.Thread(target=self._run, name=f"audio-{self.cfg.name}", daemon=True)
        t.start()
        return t

    def stop(self) -> None:
        self._stop.set()

    def stats(self) -> dict[str, Any]:
        with self._stats_lock:
            return {
                "windows": self._windows,
                "attempts": self._attempts,
                "errors": self._errors,
                "last_text_at": self._last_text_at.isoformat() if self._last_text_at else None,
            }

    # ----- internals -----

    def _emit(self, v: Verdict) -> None:
        """Thread-safe handoff onto the async outbox queue (same route the
        video readers use → outbox → POST /control/events)."""
        event = upsell_event(str(self.cfg.id), v)
        asyncio.run_coroutine_threadsafe(
            self.queue.put({"kind": "event", "payload": event}),
            self.loop,
        )

    def _flush(self, pcm: np.ndarray, sr: int, elapsed: float, last_emit: dict[str, float]) -> float:
        """Transcribe one window, classify, and emit on a confident upsell.
        Returns the new media-clock position. `pcm` is dropped by the caller."""
        window_end = elapsed + len(pcm) / sr
        text = self._stt.transcribe(pcm, sr)  # audio → text; pcm dropped after
        with self._stats_lock:
            self._windows += 1
        if not text:
            return window_end
        with self._stats_lock:
            self._last_text_at = datetime.now(timezone.utc)
        v = classify(text, client=self._intent_client)
        if v.error:
            log("warn", "upsell.classify_error", camera=self.cfg.name, error=v.error)
        if not should_emit(v, window_end, last_emit):
            return window_end
        with self._stats_lock:
            self._attempts += 1
        log("info", "upsell.attempt", camera=self.cfg.name, item=v.item,
            confidence=round(v.confidence, 3))
        self._emit(v)
        return window_end

    def _run(self) -> None:
        source = self.cfg.rtsp_url
        if not source:
            log("info", "upsell.skip_no_rtsp", camera=self.cfg.name)
            return
        af = upsell_capture.ENHANCE_AF if self._enhance else None
        sr = upsell_capture.SAMPLE_RATE
        target = int(WINDOW_SEC * sr)
        log("info", "upsell.reader_start", camera=self.cfg.name,
            window_sec=WINDOW_SEC, enhance=self._enhance)
        backoff = 1.0
        while not self._stop.is_set():
            buf: list[np.ndarray] = []
            have = 0
            elapsed = 0.0
            last_emit: dict[str, float] = {}
            try:
                for chunk in upsell_capture.stream_pcm(
                    source, sample_rate=sr, chunk_sec=1.0, af=af, stop=self._stop,
                ):
                    buf.append(chunk)
                    have += len(chunk)
                    if have >= target:
                        elapsed = self._flush(np.concatenate(buf), sr, elapsed, last_emit)
                        buf, have = [], 0
                backoff = 1.0  # clean EOF → reset backoff before reconnecting
            except Exception as e:  # noqa: BLE001 — reconnect on any stream error
                with self._stats_lock:
                    self._errors += 1
                log("warn", "upsell.stream_failed", camera=self.cfg.name,
                    error=str(e), backoff_s=backoff)
                self._stop.wait(backoff)
                backoff = min(backoff * 2, 30.0)
        log("info", "upsell.reader_stop", camera=self.cfg.name)
