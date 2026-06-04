"""Tests for ``worker.ppe_yolo``.

Focus is on:
  - The disabled-by-default path (no PPE_YOLO_MODEL → returns None,
    never imports torch/ultralytics).
  - The envelope shape exactly matches the contract documented on the
    Rust side at ``crates/cameras/src/service/compliance.rs``
    ``CompliancePayload::detections_json``. The reviewer UI depends
    on bbox coords being normalized [0,1] with top-left origin; if
    that ever drifts the overlay silently mis-positions.

We deliberately don't run the real model in unit tests — the wheels
weigh hundreds of MB and CI would have to download / cache them. The
shape-conformance test stubs the YOLO class with a fake that returns
deterministic boxes.
"""
from __future__ import annotations

import importlib
import sys
from pathlib import Path
from types import SimpleNamespace
from typing import Any

import numpy as np
import pytest

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))


def _reload_ppe_yolo(monkeypatch: pytest.MonkeyPatch, env: dict[str, str]) -> Any:
    """Reload the module under controlled env so the module-level
    constants (``PPE_YOLO_MODEL`` etc.) and the lazy singleton state
    reset between tests."""
    for k in ("PPE_YOLO_MODEL", "PPE_YOLO_CONF_THRESHOLD"):
        monkeypatch.delenv(k, raising=False)
    for k, v in env.items():
        monkeypatch.setenv(k, v)
    # Drop any cached version so module-level env reads run fresh.
    sys.modules.pop("worker.ppe_yolo", None)
    return importlib.import_module("worker.ppe_yolo")


def test_disabled_when_env_unset(monkeypatch: pytest.MonkeyPatch) -> None:
    """No PPE_YOLO_MODEL → infer() returns None and is_enabled() is False.
    No PyTorch import should be triggered — verify by asserting the
    module-level model stays None even after a call."""
    mod = _reload_ppe_yolo(monkeypatch, {})
    assert mod.is_enabled() is False
    frame = np.zeros((720, 1280, 3), dtype=np.uint8)
    assert mod.infer(frame, frame_offset_ms=1000) is None
    # Internal: the load should have been attempted exactly once and
    # short-circuited before importing ultralytics.
    assert mod._model is None  # type: ignore[attr-defined]
    assert mod._model_load_attempted is True  # type: ignore[attr-defined]


def test_envelope_shape_matches_contract(monkeypatch: pytest.MonkeyPatch) -> None:
    """When the model returns boxes, the envelope shape MUST be exactly
    what the Rust backend + frontend overlay expect:
      { model: str, frame_offset_ms: int, frame_width: int,
        frame_height: int,
        detections: [ { class: str, confidence: float,
                        bbox: { x, y, width, height } } ] }
    Bbox coords are normalized [0,1] against frame dims, top-left origin.
    """
    mod = _reload_ppe_yolo(
        monkeypatch,
        {"PPE_YOLO_MODEL": "/tmp/fake-ppe.pt", "PPE_YOLO_CONF_THRESHOLD": "0.2"},
    )

    # Fake YOLO that returns one box at (100, 50) → (300, 250) on a
    # 1280x720 frame with class 1 (label "hat") at confidence 0.83.
    fake_boxes = SimpleNamespace(
        xyxy=_as_tensor(np.array([[100.0, 50.0, 300.0, 250.0]])),
        conf=_as_tensor(np.array([0.83])),
        cls=_as_tensor(np.array([1.0])),
    )
    fake_result = SimpleNamespace(boxes=fake_boxes)

    def fake_model(_frame: np.ndarray, conf: float, verbose: bool) -> list[Any]:
        assert verbose is False
        assert conf == pytest.approx(0.2)
        return [fake_result]

    fake_model.names = {0: "person", 1: "hat", 2: "hairnet"}  # type: ignore[attr-defined]
    mod._model = fake_model  # type: ignore[attr-defined]
    mod._model_load_attempted = True  # type: ignore[attr-defined]
    mod._model_name = "yolov8n-ppe"  # type: ignore[attr-defined]

    frame = np.zeros((720, 1280, 3), dtype=np.uint8)
    env = mod.infer(frame, frame_offset_ms=4200)
    assert env is not None
    assert env["model"] == "yolov8n-ppe"
    assert env["frame_offset_ms"] == 4200
    assert env["frame_width"] == 1280
    assert env["frame_height"] == 720
    assert len(env["detections"]) == 1
    det = env["detections"][0]
    assert det["class"] == "hat"
    assert det["confidence"] == pytest.approx(0.83, abs=1e-3)
    # Bbox: (100,50,300,250) on 1280x720 →
    #   x = 100/1280 ≈ 0.07813
    #   y = 50/720   ≈ 0.06944
    #   w = 200/1280 ≈ 0.15625
    #   h = 200/720  ≈ 0.27778
    assert det["bbox"]["x"] == pytest.approx(100 / 1280, abs=1e-4)
    assert det["bbox"]["y"] == pytest.approx(50 / 720, abs=1e-4)
    assert det["bbox"]["width"] == pytest.approx(200 / 1280, abs=1e-4)
    assert det["bbox"]["height"] == pytest.approx(200 / 720, abs=1e-4)


def test_empty_detections_when_model_returns_nothing(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """YOLO running but seeing nothing is a *present* envelope with an
    empty list — distinct from None (which means "edge worker doesn't
    do PPE yet"). The UI distinguishes the two."""
    mod = _reload_ppe_yolo(monkeypatch, {"PPE_YOLO_MODEL": "/tmp/fake.pt"})

    fake_boxes = SimpleNamespace(
        xyxy=_as_tensor(np.zeros((0, 4))),
        conf=_as_tensor(np.zeros((0,))),
        cls=_as_tensor(np.zeros((0,))),
    )
    fake_result = SimpleNamespace(boxes=fake_boxes)

    def fake_model(*_args: Any, **_kwargs: Any) -> list[Any]:
        return [fake_result]

    fake_model.names = {0: "person"}  # type: ignore[attr-defined]
    mod._model = fake_model  # type: ignore[attr-defined]
    mod._model_load_attempted = True  # type: ignore[attr-defined]
    mod._model_name = "yolo-test"  # type: ignore[attr-defined]

    env = mod.infer(np.zeros((480, 640, 3), dtype=np.uint8), frame_offset_ms=0)
    assert env is not None
    assert env["detections"] == []


def test_inference_failure_degrades_gracefully(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """If YOLO raises (OOM, bad frame, etc.) we return None and let the
    compliance report ship without detections rather than failing the
    whole VLM path."""
    mod = _reload_ppe_yolo(monkeypatch, {"PPE_YOLO_MODEL": "/tmp/fake.pt"})

    def fake_model(*_args: Any, **_kwargs: Any) -> list[Any]:
        raise RuntimeError("simulated CUDA OOM")

    fake_model.names = {0: "person"}  # type: ignore[attr-defined]
    mod._model = fake_model  # type: ignore[attr-defined]
    mod._model_load_attempted = True  # type: ignore[attr-defined]
    mod._model_name = "yolo-test"  # type: ignore[attr-defined]

    env = mod.infer(np.zeros((100, 100, 3), dtype=np.uint8), frame_offset_ms=0)
    assert env is None


def test_clamps_overhanging_bboxes(monkeypatch: pytest.MonkeyPatch) -> None:
    """YOLO occasionally returns boxes that overhang the frame edge by
    1-2 px on small subjects. Normalized coords MUST stay in [0,1] —
    the UI percent-positions against the video element, and negative
    coords would render outside the player container."""
    mod = _reload_ppe_yolo(monkeypatch, {"PPE_YOLO_MODEL": "/tmp/fake.pt"})

    # Box overhangs both top-left (-5, -5) and bottom-right (650, 490)
    # on a 640x480 frame.
    fake_boxes = SimpleNamespace(
        xyxy=_as_tensor(np.array([[-5.0, -5.0, 650.0, 490.0]])),
        conf=_as_tensor(np.array([0.7])),
        cls=_as_tensor(np.array([0.0])),
    )
    fake_result = SimpleNamespace(boxes=fake_boxes)

    def fake_model(*_args: Any, **_kwargs: Any) -> list[Any]:
        return [fake_result]

    fake_model.names = {0: "person"}  # type: ignore[attr-defined]
    mod._model = fake_model  # type: ignore[attr-defined]
    mod._model_load_attempted = True  # type: ignore[attr-defined]
    mod._model_name = "yolo-test"  # type: ignore[attr-defined]

    env = mod.infer(np.zeros((480, 640, 3), dtype=np.uint8), frame_offset_ms=0)
    assert env is not None
    det = env["detections"][0]
    assert det["bbox"]["x"] >= 0.0
    assert det["bbox"]["y"] >= 0.0
    assert det["bbox"]["x"] + det["bbox"]["width"] <= 1.0
    assert det["bbox"]["y"] + det["bbox"]["height"] <= 1.0


# ---------------------------------------------------------------------------
# Helpers


class _FakeTensor:
    """Minimal stand-in for the torch.Tensor methods ppe_yolo touches.
    Avoids a torch dependency in the unit-test path."""

    def __init__(self, arr: np.ndarray) -> None:
        self._arr = arr

    def cpu(self) -> "_FakeTensor":
        return self

    def numpy(self) -> np.ndarray:
        return self._arr

    def __len__(self) -> int:
        return len(self._arr)


def _as_tensor(arr: np.ndarray) -> _FakeTensor:
    return _FakeTensor(arr)
