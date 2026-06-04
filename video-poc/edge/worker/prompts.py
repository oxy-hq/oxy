"""VLM prompt rendering driven by the active domain pack.

The backend ships the workspace's active pack inline in
`GET /control/boxes/{id}/config` (see `crates/cameras/src/routes/dto.rs`
`EdgeConfigDto`). `main._fetch_config` installs the pack here via
[`set_active_pack`] after each successful poll; [`render_prompt`]
reads it at VLM-call time so the most-recent operator edits take
effect immediately without restarting the reader.

Phase 3 (commit landing this file): the built-in `ROLE_PROMPTS`
fallback that lived here through phase 2 is gone. The pack is the
only source of truth. If a config poll succeeds but ships a
`domain_pack: null` (workspace's pack hasn't been seeded yet, or
the backend's pack fetch errored), [`render_prompt`] returns `None`
and the caller skips the VLM call. The next config poll re-fetches
and the pack auto-seeds, so this is a transient condition rather
than a hard failure.

Backward compat with phase-1 backends (no pack in the response) is
out of scope — phase 3 assumes the backend has phase-2 code. If we
need to roll back, redeploy the phase-2 worker.
"""
from __future__ import annotations

from typing import Any

from jinja2 import Environment, StrictUndefined

from .log import log

# StrictUndefined matches the backend's minijinja render validation
# (UndefinedBehavior::Strict). A typo'd `{{ variables.attir }}`
# raises rather than silently producing empty output. Backend's
# `service::packs::validate::check_pack_renders` should catch these
# at PUT time, so a render failure here means either a backend bug
# or a pack body field referenced in the template but not in the
# pack's `variables` map.
_jinja_env: Environment = Environment(
    autoescape=False, undefined=StrictUndefined, trim_blocks=False, lstrip_blocks=False
)

_active_pack: dict[str, Any] | None = None


def set_active_pack(pack: dict[str, Any] | None) -> None:
    """Install (or clear) the active domain pack. Called from
    `main._config_poller` once per successful poll. `None` clears
    — subsequent `render_prompt` calls return None until a non-null
    pack is installed."""
    global _active_pack
    _active_pack = pack


def render_prompt(role: str | None, *, track_id: int, dwell_seconds: float) -> str | None:
    """Render the VLM prompt for `role` against the active pack.

    Returns the rendered prompt, or `None` if no pack is available
    (caller should skip the VLM call and emit a warning log). A
    render exception (template syntax error, missing variable)
    also returns `None` — the worker stays running and the next
    config poll may pick up a corrected pack.

    Resolution within the active pack:
      1. Role with matching `role_key`.
      2. Role with `role_key == "other"` (operator-curated fallback).
      3. First role in the list.
    """
    pack = _active_pack
    if pack is None:
        log("warn", "prompts.no_active_pack", role=role)
        return None

    template_source, matched_role = _select_pack_template(pack, role)
    if template_source is None:
        log("warn", "prompts.empty_pack", role=role)
        return None

    try:
        context = {
            "track_id": track_id,
            "dwell_seconds": dwell_seconds,
            "variables": pack.get("variables", {}),
        }
        return _jinja_env.from_string(template_source).render(**context)
    except Exception as exc:  # jinja2 raises a subclass; catch broadly
        log(
            "warn",
            "prompts.pack_render_failed",
            role=role,
            matched_role=matched_role,
            error=str(exc),
        )
        return None


def _select_pack_template(
    pack: dict[str, Any], role: str | None
) -> tuple[str | None, str | None]:
    """Pick a role's prompt template from the pack body.

    Returns `(template_source, matched_role_key)` or `(None, None)`
    if no candidate exists. Mirrors the backend's
    `PackBody::prompt_for` resolution rules so the worker and
    backend agree on which prompt a given `role` lands on.
    """
    roles = pack.get("roles") or []
    if role is not None:
        for r in roles:
            if r.get("role_key") == role:
                return r.get("prompt"), role
    for r in roles:
        if r.get("role_key") == "other":
            return r.get("prompt"), "other"
    if roles:
        first = roles[0]
        return first.get("prompt"), first.get("role_key")
    return None, None
