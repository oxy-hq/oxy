"""End-to-end POC pipeline: audio source → STT → upsell intent → event.

  capture.stream_pcm  →  window buffer  →  transcribe (discard audio)
                                              →  intent.classify
                                              →  emit `upsell_attempt` event

The emitted event mirrors the edge `_event_dict` shape so Phase 2 just swaps
`_emit` for a `POST /control/events`. For the POC it prints the event JSON.

Transcribe-and-discard: each PCM window is transcribed then dropped; no audio
or transcript is persisted.
"""
from __future__ import annotations

import uuid
from datetime import datetime, timezone

import anthropic
import numpy as np

import capture
import emit as emit_
import intent
import transcribe

CONF_THRESHOLD = 0.5
# Don't re-emit the same item within this window (one offer can straddle two
# transcription windows; this collapses the duplicate).
ITEM_COOLDOWN_SEC = 30.0


def run_source(source: str, *, camera_id: str, window_sec: float = 12.0,
               threshold: float = CONF_THRESHOLD, sink=None,
               duration_sec: float | None = None, enhance: bool = False) -> int:
    sink = sink or emit_.make_sink("print")
    af = capture.ENHANCE_AF if enhance else None
    tr = transcribe.Transcriber()
    client = anthropic.Anthropic()
    sr = capture.SAMPLE_RATE
    target = int(window_sec * sr)

    buf: list[np.ndarray] = []
    have = 0
    state = {"elapsed": 0.0, "last_emit": {}, "n": 0}  # media-clock + dedup + count

    def flush(pcm: np.ndarray) -> None:
        window_end = state["elapsed"] + len(pcm) / sr
        state["elapsed"] = window_end
        text = tr.transcribe(pcm, sr)  # audio → text; the caller drops `pcm` after
        if not text:
            return
        print(f"    [stt ~{window_end:5.0f}s] {text}")
        v = intent.classify(text, client=client)
        if not (v.is_upsell and v.confidence >= threshold):
            return
        key = (v.item or "unknown").lower()
        if window_end - state["last_emit"].get(key, -1e9) < ITEM_COOLDOWN_SEC:
            return
        state["last_emit"][key] = window_end
        state["n"] += 1
        sink(_event(camera_id, v), evidence=v.evidence, at=window_end)

    for chunk in capture.stream_pcm(source, sample_rate=sr, chunk_sec=1.0,
                                    duration_sec=duration_sec, af=af):
        buf.append(chunk)
        have += len(chunk)
        if have >= target:
            flush(np.concatenate(buf))
            buf, have = [], 0
    if buf:                            # flush the final partial window
        flush(np.concatenate(buf))

    print(f"\n[done] {state['n']} upsell attempt(s) detected from {source}")
    return 0


def _event(camera_id: str, v: intent.Verdict) -> dict:
    """The wire event — same shape as the edge `_event_dict`; `label` carries
    the offered item (spec §5), `confidence` the classifier score."""
    return {
        "event_id": str(uuid.uuid4()),
        "ts": datetime.now(timezone.utc).isoformat(),
        "camera_id": camera_id,
        "event_type": "upsell_attempt",
        "label": v.item,
        "confidence": round(v.confidence, 3),
    }
