"""PPE-YOLO inference (Option C — visual hints alongside VLM verdicts).

Runs at the same trigger point as the VLM check — once per ~minute per
camera on the frame the VLM scores — so the cost shape mirrors the
existing two-stage pipeline (cheap-per-frame YOLO11n + sparse on-trigger).
Designed to be **off by default**: the operator picks a model (typically
from Roboflow Universe), mounts the `.pt` file into the container, and
points ``PPE_YOLO_MODEL`` at it. With no env var set, this module is a
no-op and compliance reports ship without ``detections_json`` — which
the server-side contract treats as "edge worker doesn't run YOLO yet".

The data contract is enforced in
``crates/cameras/src/service/compliance.rs::CompliancePayload``; keep
the envelope shape this module returns in sync with that docstring.

Why a second YOLO model (vs reusing ``YOLO11n`` from camera.py):
the per-frame tracker uses the COCO-pretrained ``yolo11n.pt`` which
only knows ``person`` (class 0). PPE classes (hat, hairnet, apron,
glove, mask) need a model trained on a PPE-labeled dataset. We
deliberately don't bundle weights in the image (licensing varies
on Roboflow Universe; bandwidth on slow customer links matters).

Env config:

  PPE_YOLO_MODEL           Path or model id Ultralytics can load.
                           Empty / unset → PPE inference disabled.
                           Examples:
                             /app/models/yolov8n-ppe.pt    (mounted by operator)
                             yolov8n-ppe.pt                 (auto-download)
  PPE_YOLO_CONF_THRESHOLD  Minimum confidence to include in the
                           envelope (default 0.25 — the UI color-codes
                           low-confidence in amber, so we keep the
                           floor permissive and let the reviewer judge).
"""
from __future__ import annotations

import os
import threading
from typing import Any

import numpy as np

from .log import log

# ---------------------------------------------------------------------------
# Env-driven config

PPE_YOLO_MODEL = os.environ.get("PPE_YOLO_MODEL", "").strip()
PPE_YOLO_CONF_THRESHOLD = float(os.environ.get("PPE_YOLO_CONF_THRESHOLD", "0.25"))

# ---------------------------------------------------------------------------
# Module-level lazy singleton.
#
# Ultralytics' ``YOLO(...)`` is a few hundred ms on first load (PyTorch
# import + weights mmap). We share one instance across cameras — model
# inference is stateless and the trigger rate is per-minute, so there's
# no contention worth a per-camera instance.
#
# Loading is deferred until the first ``infer()`` call: that way an
# unconfigured deployment never touches PyTorch / Ultralytics and the
# main process doesn't pay the cold-start hit unless we'll actually use
# the model.

_model_lock = threading.Lock()
_model: Any = None
_model_load_attempted = False
_model_name: str = ""


def _load_model_once() -> Any:
    """Returns the Ultralytics YOLO model, or None when PPE inference
    is disabled or the load failed. The failure is logged once; subsequent
    calls short-circuit without re-trying so a misconfigured path doesn't
    spam the log every trigger."""
    global _model, _model_load_attempted, _model_name
    if _model is not None:
        return _model
    with _model_lock:
        if _model is not None:
            return _model
        if _model_load_attempted:
            return None
        _model_load_attempted = True
        if not PPE_YOLO_MODEL:
            log("info", "ppe_yolo.disabled", reason="PPE_YOLO_MODEL not set")
            return None
        try:
            from ultralytics import YOLO  # type: ignore[import-untyped]

            model = YOLO(PPE_YOLO_MODEL)
            # Cache the derived "model name" we want to record on every
            # report. Strip the path so the envelope says e.g.
            # "yolov8n-ppe" rather than "/app/models/yolov8n-ppe.pt"
            # — the latter leaks deployment-path detail into customer
            # analytics for no benefit.
            stem = os.path.basename(PPE_YOLO_MODEL)
            if stem.endswith(".pt"):
                stem = stem[: -len(".pt")]
            _model_name = stem or "ppe-yolo"
            _model = model
            log("info", "ppe_yolo.loaded", model=_model_name, path=PPE_YOLO_MODEL)
            return model
        except Exception as exc:  # noqa: BLE001 — log + degrade
            log(
                "warn",
                "ppe_yolo.load_failed",
                path=PPE_YOLO_MODEL,
                error=str(exc),
            )
            return None


def is_enabled() -> bool:
    """Cheap check the caller can use to skip work when PPE is off."""
    return bool(PPE_YOLO_MODEL)


def infer(frame: np.ndarray, frame_offset_ms: int) -> dict[str, Any] | None:
    """Run PPE detection on ``frame`` and return the envelope the
    backend expects on ``CompliancePayload.detections_json``.

    Returns ``None`` (NOT an empty envelope) when:
      - PPE inference is disabled (no env var)
      - Model failed to load
      - Inference raised

    An empty ``detections`` array inside a present envelope means
    "YOLO ran on this frame, saw nothing meaningful above the
    confidence floor" — which is distinct from "edge worker doesn't
    do PPE yet" and the reviewer UI distinguishes the two states.
    """
    model = _load_model_once()
    if model is None:
        return None
    if frame is None or frame.size == 0:
        return None
    height, width = frame.shape[:2]
    try:
        # ``verbose=False`` silences Ultralytics' per-call CLI banner.
        # Single-image call returns a list of length 1.
        results = model(frame, conf=PPE_YOLO_CONF_THRESHOLD, verbose=False)
    except Exception as exc:  # noqa: BLE001 — log + degrade
        log("warn", "ppe_yolo.inference_failed", error=str(exc))
        return None
    if not results:
        return _envelope(width, height, frame_offset_ms, [])
    res = results[0]
    boxes = getattr(res, "boxes", None)
    if boxes is None or boxes.xyxy is None:
        return _envelope(width, height, frame_offset_ms, [])

    # Pull tensors out as numpy + the model's class-id → name map.
    # ``.cpu().numpy()`` handles both CPU + GPU backends transparently.
    xyxy = boxes.xyxy.cpu().numpy()
    confs = boxes.conf.cpu().numpy() if boxes.conf is not None else np.zeros(len(xyxy))
    cls_ids = boxes.cls.cpu().numpy().astype(int) if boxes.cls is not None else []
    names = getattr(model, "names", {}) or {}

    detections: list[dict[str, Any]] = []
    for box, conf, cls_id in zip(xyxy, confs, cls_ids):
        x1, y1, x2, y2 = float(box[0]), float(box[1]), float(box[2]), float(box[3])
        # Normalize to [0,1] against frame dims. Clamp defensively —
        # YOLO occasionally returns boxes that overhang the frame by
        # 1-2 px on small subjects.
        nx = max(0.0, min(1.0, x1 / width))
        ny = max(0.0, min(1.0, y1 / height))
        nw = max(0.0, min(1.0, (x2 - x1) / width))
        nh = max(0.0, min(1.0, (y2 - y1) / height))
        if nw <= 0 or nh <= 0:
            continue
        detections.append(
            {
                "class": str(names.get(int(cls_id), f"class_{int(cls_id)}")),
                "confidence": round(float(conf), 4),
                "bbox": {
                    "x": round(nx, 5),
                    "y": round(ny, 5),
                    "width": round(nw, 5),
                    "height": round(nh, 5),
                },
            }
        )
    return _envelope(width, height, frame_offset_ms, detections)


def _envelope(
    width: int,
    height: int,
    frame_offset_ms: int,
    detections: list[dict[str, Any]],
) -> dict[str, Any]:
    return {
        "model": _model_name,
        "frame_offset_ms": int(frame_offset_ms),
        "frame_width": int(width),
        "frame_height": int(height),
        "detections": detections,
    }
