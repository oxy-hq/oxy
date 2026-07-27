"""Tests for ``worker.congestion.CongestionDetector`` — the sustained-occupancy
state machine. Pure logic (no YOLO, no network), driven by synthetic
(count, timestamp) frames so we can exercise the threshold / sustain / cooldown
/ flicker-debounce behaviour deterministically."""
from __future__ import annotations

import sys
from datetime import datetime, timedelta, timezone
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from worker.congestion import CongestionDetector, CongestionFlag  # noqa: E402

T0 = datetime(2026, 7, 27, 12, 0, 0, tzinfo=timezone.utc)


def _det(**kw) -> CongestionDetector:
    # Small, explicit thresholds so tests read in seconds, not minutes.
    defaults = dict(min_count=4, sustain_sec=60, clear_sec=15, enabled=True)
    defaults.update(kw)
    return CongestionDetector(**defaults)


def _feed(det, zone, count, secs):
    return det.update(zone, count, T0 + timedelta(seconds=secs))


def test_flags_only_after_sustained_backup() -> None:
    det = _det()
    # Below threshold → never flags.
    assert _feed(det, "z", 3, 0) is None
    # Crosses threshold, but not yet sustained.
    assert _feed(det, "z", 5, 10) is None
    assert _feed(det, "z", 5, 60) is None  # 50s < 60s sustain
    # Crosses the sustain threshold → flag, with the elapsed sustain.
    flag = _feed(det, "z", 6, 71)
    assert isinstance(flag, CongestionFlag)
    assert flag.zone_id == "z"
    assert flag.count == 6
    assert flag.sustained_sec == 61.0


def test_sustained_episode_flags_once() -> None:
    """One long backup → exactly one clip, no matter how long it lasts (as long
    as it never clears). This is the "one rush, one clip" guarantee."""
    det = _det()
    _feed(det, "z", 5, 0)
    assert _feed(det, "z", 5, 61) is not None  # first (and only) flag
    for t in (120, 300, 600, 1200, 3600):
        assert _feed(det, "z", 5, t) is None  # still up, already clipped this episode


def test_distinct_episode_after_clear_flags_again() -> None:
    """A genuinely separate backup that starts soon after the previous flag —
    but with a real clear in between — must still flag (no global rate limit)."""
    det = _det()
    _feed(det, "z", 5, 0)
    assert _feed(det, "z", 5, 61) is not None  # episode A flags
    _feed(det, "z", 0, 80)  # 19s below >= clear(15) → episode A ends
    _feed(det, "z", 5, 100)  # episode B begins
    assert _feed(det, "z", 5, 161) is not None  # B sustained 61s → flags again


def test_brief_dip_does_not_reset_timer() -> None:
    """A short sub-threshold blip (detection flicker) must NOT restart the
    sustain timer — the backup is still there."""
    det = _det()
    _feed(det, "z", 5, 0)  # backup starts at t=0
    _feed(det, "z", 5, 50)  # still up; last_above=50
    _feed(det, "z", 2, 55)  # dip, only 5s since last_above < clear(15) → timer holds
    flag = _feed(det, "z", 5, 65)  # 65s since since=0 ≥ 60 sustain → flag
    assert flag is not None
    assert flag.sustained_sec == 65.0


def test_sustained_dip_clears_and_restarts() -> None:
    det = _det()
    _feed(det, "z", 5, 0)  # backup starts
    _feed(det, "z", 0, 20)  # 20s below ≥ clear(15) → cleared
    assert _feed(det, "z", 5, 30) is None  # restarts; since=30
    assert _feed(det, "z", 5, 85) is None  # 55s < 60 sustain
    assert _feed(det, "z", 5, 95) is not None  # 65s ≥ 60 → flag


def test_zones_are_independent() -> None:
    det = _det()
    _feed(det, "a", 5, 0)
    _feed(det, "b", 5, 30)
    assert _feed(det, "a", 5, 61) is not None  # a sustained 61s
    assert _feed(det, "b", 5, 61) is None  # b only 31s
    assert _feed(det, "b", 5, 91) is not None  # b now 61s


def test_disabled_never_flags() -> None:
    det = _det(enabled=False)
    for s in range(0, 400, 10):
        assert _feed(det, "z", 10, s) is None
