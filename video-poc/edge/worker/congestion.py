"""Sustained-occupancy ("congestion") detection per zone.

A pure state machine, deliberately separate from CameraReader so it unit-tests
without loading YOLO or touching the network. Feed it `(zone_id, count, ts)`
once per frame — `count` is the number of tracked people currently inside the
zone, which `_handle_zones` already computes — and it returns a `CongestionFlag`
the moment a zone has been backed up long enough to be worth a clip.

Why this signal (not per-person tracking or appearance): the per-frame count of
people in the zone is reliable and churn-robust even though it comes from
tracker ids — a person is one id in one frame, and it's id *continuity* across
frames (which churns ~31x) that we don't depend on. Per-person identity and
guest/staff appearance are NOT reliable here (see the time-to-serve
investigation), and the `service_area` zone already separates the customer side
from staff geometrically. So congestion = a sustained high head-count in the
customer zone.

Rule, per zone — one clip per backup *episode*:
  • flag ONCE when count >= MIN_COUNT continuously for >= SUSTAIN_SEC. It won't
    fire again for the same episode no matter how long it lasts (one long rush
    is one clip, not a storm).
  • the episode ends when count stays < MIN_COUNT for >= CLEAR_SEC; brief
    sub-threshold dips below that don't reset the timer (a single
    missed-detection frame shouldn't restart a 5-minute backup).
  • a genuinely distinct backup after a clear flags again right away — there's
    no global rate limit, so separate episodes each get their own evidence.
"""
from __future__ import annotations

import os
from collections import defaultdict
from dataclasses import dataclass
from datetime import datetime

# Env-driven tunables (per-store calibration, same convention as camera.py).
CONGESTION_ENABLED = os.environ.get("CONGESTION_ENABLED", "1") != "0"
CONGESTION_MIN_COUNT = int(os.environ.get("CONGESTION_MIN_COUNT", "4"))
CONGESTION_SUSTAIN_SEC = float(os.environ.get("CONGESTION_SUSTAIN_SEC", "300"))
# How long count must stay BELOW the threshold before the backup counts as
# cleared. Debounces detection flicker so one dropped frame doesn't reset the
# sustain timer — and gates when a fresh episode (and its next clip) can begin.
CONGESTION_CLEAR_SEC = float(os.environ.get("CONGESTION_CLEAR_SEC", "15"))
# Length of the evidence clip pulled from the recording (ending at flag time).
CONGESTION_CLIP_SEC = float(os.environ.get("CONGESTION_CLIP_SEC", "30"))


@dataclass
class _ZoneState:
    since: datetime | None = None  # when the current backup episode began
    last_above: datetime | None = None  # last frame at/above threshold
    flagged: bool = False  # already clipped THIS episode?


@dataclass
class CongestionFlag:
    zone_id: str
    count: int
    sustained_sec: float


class CongestionDetector:
    """Per-zone sustained-occupancy detector. Stateful across frames; pure
    (no I/O). One instance per camera. Fires once per backup episode."""

    def __init__(
        self,
        *,
        min_count: int = CONGESTION_MIN_COUNT,
        sustain_sec: float = CONGESTION_SUSTAIN_SEC,
        clear_sec: float = CONGESTION_CLEAR_SEC,
        enabled: bool = CONGESTION_ENABLED,
    ) -> None:
        self._min = min_count
        self._sustain = sustain_sec
        self._clear = clear_sec
        self._enabled = enabled
        self._zones: dict[str, _ZoneState] = defaultdict(_ZoneState)

    def update(self, zone_id: str, count: int, ts: datetime) -> CongestionFlag | None:
        """Advance the state machine for one frame. Returns a `CongestionFlag`
        exactly on the frame an episode first crosses the sustain threshold,
        then `None` for the rest of that episode."""
        if not self._enabled:
            return None
        st = self._zones[zone_id]

        if count >= self._min:
            st.last_above = ts
            if st.since is None:
                # A fresh episode begins — reset the once-per-episode latch.
                st.since = ts
                st.flagged = False
        else:
            # Below threshold: only end the episode once we've been below for
            # the grace period (flicker debounce), then stop.
            if st.last_above is None or (ts - st.last_above).total_seconds() >= self._clear:
                st.since = None
                st.flagged = False
            return None

        if st.since is None or st.flagged:
            return None
        sustained = (ts - st.since).total_seconds()
        if sustained < self._sustain:
            return None

        st.flagged = True  # latch — no more flags until this episode clears
        return CongestionFlag(zone_id=zone_id, count=count, sustained_sec=sustained)
