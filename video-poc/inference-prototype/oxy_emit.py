"""Thin client to push detections + compliance reports from this notebook
into the oxy-internal video-poc control-api.

The notebook stays the same on the input side (reads frames from an MP4); the
output side now writes to the central Postgres via the control-api instead of
to local CSV/JSONL files. Once we're happy with the schema, the same emit
calls will be made by the production edge worker.

Usage:
    from oxy_emit import OxyEmitter

    emit = OxyEmitter()
    emit.register()                                 # one-time per session
    cam = emit.get_camera("video-lm-test")          # pre-seeded in init.sql
    emit.emit_event(cam.id, "enter", track_id="42", zone_id="ppe_zone")
    emit.emit_compliance_report(cam.id, ...)
    emit.flush()                                    # send any batched events
"""
from __future__ import annotations

import os
import uuid
from dataclasses import dataclass
from datetime import datetime, timezone
from typing import Any

import httpx


DEFAULT_BASE_URL = os.environ.get("OXY_CONTROL_API", "http://localhost:8080")
DEFAULT_HARDWARE_ID = "video-lm-notebook"


@dataclass
class Camera:
    id: uuid.UUID
    name: str
    zones_json: list[dict[str, Any]]
    lines_json: list[dict[str, Any]]


class OxyEmitter:
    def __init__(
        self,
        base_url: str = DEFAULT_BASE_URL,
        hardware_id: str = DEFAULT_HARDWARE_ID,
        batch_size: int = 50,
    ) -> None:
        self._base_url = base_url.rstrip("/")
        self._hardware_id = hardware_id
        self._batch_size = batch_size
        self._client = httpx.Client(base_url=self._base_url, timeout=15.0)

        self.box_id: uuid.UUID | None = None
        self.site_id: uuid.UUID | None = None
        self._cameras: dict[str, Camera] = {}
        self._event_buffer: list[dict[str, Any]] = []

    # ------------------------------------------------------------------
    # Lifecycle
    # ------------------------------------------------------------------

    def register(self) -> None:
        r = self._client.post(
            "/control/register",
            json={
                "hardware_id": self._hardware_id,
                "hardware_model": "notebook-host",
                "bootstrap_token": "poc-dev-token",
            },
        )
        r.raise_for_status()
        data = r.json()
        self.box_id = uuid.UUID(data["box_id"])
        self.site_id = uuid.UUID(data["site_id"]) if data["site_id"] else None

        # Cache camera configs assigned to us.
        cfg = self._client.get(f"/control/boxes/{self.box_id}/config")
        cfg.raise_for_status()
        for c in cfg.json():
            cam = Camera(
                id=uuid.UUID(c["id"]),
                name=c["name"],
                zones_json=c.get("zones_json") or [],
                lines_json=c.get("lines_json") or [],
            )
            self._cameras[cam.name] = cam

    def get_camera(self, name: str) -> Camera:
        if not self._cameras:
            raise RuntimeError("call register() first")
        if name not in self._cameras:
            available = ", ".join(sorted(self._cameras)) or "(none)"
            raise KeyError(
                f"no camera {name!r} assigned to this edge — available: {available}. "
                "Add a row to the cameras table or change the name."
            )
        return self._cameras[name]

    def close(self) -> None:
        self.flush()
        self._client.close()

    def __enter__(self) -> "OxyEmitter":
        self.register()
        return self

    def __exit__(self, *_exc: object) -> None:
        self.close()

    # ------------------------------------------------------------------
    # Events
    # ------------------------------------------------------------------

    def emit_event(
        self,
        camera_id: uuid.UUID,
        event_type: str,
        track_id: str,
        *,
        ts: datetime | None = None,
        zone_id: str | None = None,
        line_id: str | None = None,
        dwell_seconds: float | None = None,
        confidence: float | None = None,
        frame_uri: str | None = None,
    ) -> None:
        """Queue one event. Flushes automatically when the batch is full."""
        event = {
            "event_id": str(uuid.uuid4()),
            "ts": (ts or datetime.now(timezone.utc)).isoformat(),
            "camera_id": str(camera_id),
            "event_type": event_type,
            "zone_id": zone_id,
            "line_id": line_id,
            "track_id": str(track_id),
            "dwell_seconds": dwell_seconds,
            "confidence": confidence,
            "frame_uri": frame_uri,
        }
        self._event_buffer.append(event)
        if len(self._event_buffer) >= self._batch_size:
            self.flush()

    def flush(self) -> None:
        if not self._event_buffer:
            return
        batch = self._event_buffer
        self._event_buffer = []
        r = self._client.post("/control/events", json=batch)
        r.raise_for_status()

    # ------------------------------------------------------------------
    # Compliance reports
    # ------------------------------------------------------------------

    def emit_compliance_report(
        self,
        camera_id: uuid.UUID,
        *,
        segment_start: datetime,
        segment_end: datetime,
        trigger_type: str,
        vlm_model: str,
        report_text: str,
        trigger_track_id: str | None = None,
        structured_json: dict[str, Any] | None = None,
        frame_uri: str | None = None,
        tokens_used: int | None = None,
    ) -> None:
        """Push one compliance report directly (no buffering — they're rare)."""
        payload = [{
            "report_id": str(uuid.uuid4()),
            "camera_id": str(camera_id),
            "segment_start": segment_start.isoformat(),
            "segment_end": segment_end.isoformat(),
            "trigger_type": trigger_type,
            "trigger_track_id": str(trigger_track_id) if trigger_track_id is not None else None,
            "vlm_model": vlm_model,
            "report_text": report_text,
            "structured_json": structured_json or {},
            "frame_uri": frame_uri,
            "tokens_used": tokens_used,
        }]
        r = self._client.post("/control/compliance-reports", json=payload)
        r.raise_for_status()

    # ------------------------------------------------------------------
    # Zone calibration (hybrid flow: VLM suggests, operator applies)
    # ------------------------------------------------------------------

    def suggest_zones(
        self,
        camera_id: uuid.UUID,
        *,
        zones_json: list[dict[str, Any]] | None = None,
        lines_json: list[dict[str, Any]] | None = None,
    ) -> None:
        """PATCH the camera's zones/lines in central Postgres.

        For the hybrid flow this is called AFTER an operator approves the
        VLM's suggestion. Edges will pick up the change on next config poll.
        """
        body: dict[str, Any] = {}
        if zones_json is not None:
            body["zones_json"] = zones_json
        if lines_json is not None:
            body["lines_json"] = lines_json
        r = self._client.patch(f"/control/cameras/{camera_id}/zones", json=body)
        r.raise_for_status()
