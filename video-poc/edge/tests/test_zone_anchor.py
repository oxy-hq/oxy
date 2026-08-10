"""Regression guard for the zone-count anchor choice.

`worker/camera.py` and `worker/annotate.py` build `sv.PolygonZone` with
`triggering_anchors=(sv.Position.CENTER,)`. Supervision's default is
`BOTTOM_CENTER` (feet), which undercounts people whose feet are occluded
behind a counter: their detection box-bottom lands at/below the zone edge,
so they read as out-of-zone (grey box, uncounted) even though they're
plainly inside it. Anchoring on the box CENTER fixes that.

This pins the behavior so a supervision version bump or an accidental
revert to the default anchor is caught.
"""
from __future__ import annotations

import numpy as np
import supervision as sv


def _occluded_feet_case() -> tuple[np.ndarray, sv.Detections]:
    # Zone covers y 0..70. A person box with feet at y=90 (occluded, below the
    # zone edge) but center at y=60 (inside the zone) — the top-left case.
    poly = np.array([[0, 0], [100, 0], [100, 70], [0, 70]])
    det = sv.Detections(xyxy=np.array([[40.0, 30.0, 60.0, 90.0]]), class_id=np.array([0]))
    return poly, det


def test_center_anchor_counts_occluded_feet() -> None:
    poly, det = _occluded_feet_case()
    zone = sv.PolygonZone(polygon=poly, triggering_anchors=(sv.Position.CENTER,))
    assert bool(zone.trigger(det)[0]) is True


def test_default_feet_anchor_would_miss_it() -> None:
    # Documents the bug: the default BOTTOM_CENTER anchor drops the person.
    poly, det = _occluded_feet_case()
    zone = sv.PolygonZone(polygon=poly, triggering_anchors=(sv.Position.BOTTOM_CENTER,))
    assert bool(zone.trigger(det)[0]) is False
