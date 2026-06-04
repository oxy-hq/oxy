"""Wire-shape models mirroring oxy-cameras' route DTOs."""
from __future__ import annotations

from typing import Any
from uuid import UUID

from pydantic import BaseModel


class CameraConfig(BaseModel):
    """One row from GET /control/boxes/{id}/config."""

    id: UUID
    name: str
    # Optional — may be empty until the operator fetches the rtspAlias
    # via UniFi onboarding or pastes one in manually.
    rtsp_url: str | None = None
    substream_url: str | None = None
    zones_json: list[dict[str, Any]] = []
    lines_json: list[dict[str, Any]] = []
    analytics_consent: bool = True
    # Free-text role used to pick a VLM prompt (kitchen / prep / dining
    # / other). None falls back to the general whole-frame prompt; the
    # worker treats this as an opaque key so adding a new role on the
    # backend doesn't require a worker deploy.
    role: str | None = None


class EdgeConfig(BaseModel):
    """Full response shape for GET /control/boxes/{id}/config.

    Wraps the camera list with the workspace's active domain pack.
    The pack carries the Jinja2 prompt templates keyed by role —
    `prompts.set_active_pack` reads it after each successful poll
    so VLM calls render the operator's most-recent prompts.

    `domain_pack` is None during the brief window between workspace
    creation and the first pack auto-seed (or if pack fetching
    errored on the backend); the worker falls back to its built-in
    prompts in that window.
    """

    cameras: list[CameraConfig] = []
    domain_pack: dict[str, Any] | None = None
    # Tier B #6 — workspace the box belongs to. Used for path-
    # partitioning clip uploads in S3. Optional in the model so
    # older backends that don't ship it still parse cleanly; the
    # worker degrades to no-archive when this is None.
    workspace_id: str | None = None
