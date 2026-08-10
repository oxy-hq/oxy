"""Tests for ``worker.upsell`` — the audio upsell-detection orchestration.

Pure logic only (no ffmpeg, no STT model, no network): the UPSELL_CAMERAS
allowlist parsing, the per-item cooldown/threshold emit gate, and the
wire-event shape. STT + the Haiku classifier are external boundaries,
exercised in the POC eval harness (video-poc/audio-poc), not here.
"""
from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from worker.upsell import (  # noqa: E402
    camera_enabled,
    should_emit,
    upsell_cameras,
    upsell_event,
)
from worker.upsell_intent import Verdict  # noqa: E402


def test_upsell_cameras_parsing(monkeypatch) -> None:
    monkeypatch.delenv("UPSELL_CAMERAS", raising=False)
    assert upsell_cameras() == []
    monkeypatch.setenv("UPSELL_CAMERAS", " Register , PH-ALMADEN-Register ,")
    assert upsell_cameras() == ["register", "ph-almaden-register"]


def test_camera_enabled_substring_case_insensitive() -> None:
    allow = ["register"]
    assert camera_enabled("PH - Santa Clara - Register", allow)
    assert camera_enabled("ph-almaden-register", allow)
    assert not camera_enabled("Kitchen 1", allow)
    assert not camera_enabled("Lobby 2", [])  # empty allowlist ⇒ never enabled


def _up(item: str = "avocado", conf: float = 0.9) -> Verdict:
    return Verdict(is_upsell=True, item=item, evidence="add avocado?", confidence=conf)


def test_should_emit_threshold() -> None:
    last: dict[str, float] = {}
    # Below threshold → no emit.
    assert not should_emit(_up(conf=0.4), 12.0, last, threshold=0.5, cooldown=30)
    # Not an upsell → no emit even at high confidence.
    assert not should_emit(Verdict(False, None, None, 0.99), 12.0, last, threshold=0.5, cooldown=30)
    # Confident upsell → emit.
    assert should_emit(_up(conf=0.9), 12.0, last, threshold=0.5, cooldown=30)


def test_should_emit_cooldown_dedup() -> None:
    last: dict[str, float] = {}
    assert should_emit(_up(), 12.0, last, cooldown=30)       # first offer
    assert not should_emit(_up(), 24.0, last, cooldown=30)   # same item within cooldown
    assert should_emit(_up(), 50.0, last, cooldown=30)       # same item, cooldown elapsed
    # A different item is never suppressed by another item's cooldown.
    assert should_emit(_up(item="salmon"), 51.0, last, cooldown=30)


def test_upsell_event_shape() -> None:
    ev = upsell_event("cam-123", _up(item="avocado", conf=0.912))
    assert ev["event_type"] == "upsell_attempt"
    assert ev["camera_id"] == "cam-123"
    assert ev["label"] == "avocado"          # offered item → oxy_cam_events.label
    assert ev["confidence"] == 0.912
    assert ev["track_id"] == ""              # matches camera.py _event_dict defaults
    assert "event_id" in ev and "ts" in ev
    # The outbox keys on event_id; every camera-event field the server
    # expects must be present (mirrors _event_dict).
    for k in ("zone_id", "line_id", "dwell_seconds", "frame_uri"):
        assert k in ev
