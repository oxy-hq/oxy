"""Update-agent watchdog + held_until enforcement tests.

The agent's tick logic is pulled out as a separate `_tick`
function exactly so these tests can drive it deterministically:
fake httpx client returning canned targets, fake `apply_update`
that records calls instead of touching docker, fake clock for
the held_until window.

Covers Tier B items #156 (watchdog rollback) and #157 (held_until
enforcement).
"""
from __future__ import annotations

import asyncio
import sys
from datetime import datetime, timedelta, timezone
from pathlib import Path
from typing import Any
from unittest.mock import patch

# Make the worker package importable when running tests via
# `python -m pytest` from the repo root.
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from worker import update_agent
from worker.update_agent import (
    AgentConfig,
    _WatchdogState,
    _parse_held_until,
    _tick,
)


def _cfg() -> AgentConfig:
    return AgentConfig(
        oxy_url="http://oxy.local",
        edge_box_id="00000000-0000-0000-0000-000000000001",
        edge_box_token="t",
        worker_service="worker",
        compose_file=None,
        poll_secs=300,
    )


class _FakeClient:
    """Captures POST bodies + replays canned GET responses."""

    def __init__(self, target_responses: list[dict[str, Any]]):
        self._targets = list(target_responses)
        self.posts: list[dict[str, Any]] = []

    async def get(self, url: str, headers=None, **_kw):
        return _FakeResponse(200, self._targets.pop(0))

    async def post(self, url: str, headers=None, json=None, **_kw):
        self.posts.append({"url": url, "json": json})
        return _FakeResponse(200, {})


class _FakeResponse:
    def __init__(self, status_code: int, body: dict[str, Any]):
        self.status_code = status_code
        self._body = body

    def json(self):
        return self._body


def _run(coro):
    return asyncio.get_event_loop().run_until_complete(coro)


# ── #157 held_until ──────────────────────────────────────────────────────────


def test_held_until_in_future_skips_apply():
    """Operator set a hold window; the agent must NOT touch the box."""
    held = (datetime.now(timezone.utc) + timedelta(hours=2)).isoformat()
    client = _FakeClient([
        {
            "target_image_tag": "oxy:v2",
            "current_image_tag": "oxy:v1",
            "held_until": held,
        }
    ])
    applied: list[str] = []
    with patch.object(update_agent, "apply_update", lambda cfg, tag: applied.append(tag) or True):
        state = _WatchdogState()
        _run(_tick(client, _cfg(), state))
    assert applied == []
    assert state.pending_target is None


def test_held_until_in_past_does_not_block_apply():
    """A hold that already expired must NOT prevent an apply."""
    held = (datetime.now(timezone.utc) - timedelta(hours=2)).isoformat()
    client = _FakeClient([
        {
            "target_image_tag": "oxy:v2",
            "current_image_tag": "oxy:v1",
            "held_until": held,
        }
    ])
    applied: list[str] = []
    with patch.object(update_agent, "apply_update", lambda cfg, tag: applied.append(tag) or True):
        state = _WatchdogState()
        _run(_tick(client, _cfg(), state))
    assert applied == ["oxy:v2"]
    assert state.pending_target == "oxy:v2"


def test_parse_held_until_tolerates_z_suffix():
    """RFC 3339 with `Z` should parse — older Pythons reject it."""
    parsed = _parse_held_until("2026-12-31T23:59:59Z")
    assert parsed is not None
    assert parsed.tzinfo is not None


def test_parse_held_until_returns_none_on_garbage():
    """A non-parseable value silently disables the hold — we'd
    rather over-update than refuse forever on bad input."""
    assert _parse_held_until("not a date") is None
    assert _parse_held_until(None) is None
    assert _parse_held_until(123) is None


# ── #156 watchdog ────────────────────────────────────────────────────────────


def test_watchdog_arms_after_successful_apply():
    """After a clean apply, pending_target = new tag and the
    rollback baseline is the previous current_image_tag."""
    client = _FakeClient([
        {
            "target_image_tag": "oxy:v2",
            "current_image_tag": "oxy:v1",
            "held_until": None,
        }
    ])
    with patch.object(update_agent, "apply_update", lambda cfg, tag: True):
        state = _WatchdogState()
        _run(_tick(client, _cfg(), state))
    assert state.pending_target == "oxy:v2"
    assert state.rollback_to == "oxy:v1"
    assert state.failure_count == 0


def test_watchdog_clears_on_convergence():
    """Once current catches up to pending_target, the watchdog
    relaxes and a fresh apply can be armed later."""
    client = _FakeClient([
        {
            "target_image_tag": "oxy:v2",
            "current_image_tag": "oxy:v2",
            "held_until": None,
        }
    ])
    state = _WatchdogState(
        pending_target="oxy:v2",
        rollback_to="oxy:v1",
        failure_count=1,
    )
    with patch.object(update_agent, "apply_update", lambda cfg, tag: True):
        _run(_tick(client, _cfg(), state))
    assert state.pending_target is None
    assert state.rollback_to is None
    assert state.failure_count == 0


def test_watchdog_increments_when_current_lags():
    """Each poll without convergence ticks the counter."""
    target = {
        "target_image_tag": "oxy:v2",
        "current_image_tag": "oxy:v1",
        "held_until": None,
    }
    client = _FakeClient([target])
    state = _WatchdogState(
        pending_target="oxy:v2",
        rollback_to="oxy:v1",
        failure_count=0,
    )
    with patch.object(update_agent, "apply_update", lambda cfg, tag: True):
        _run(_tick(client, _cfg(), state))
    assert state.failure_count == 1
    assert state.pending_target == "oxy:v2"


def test_watchdog_triggers_rollback_at_threshold():
    """Crossing the threshold fires _rollback: re-applies the
    baseline, reports `reverted` to the server, resets state."""
    client = _FakeClient([
        {
            "target_image_tag": "oxy:v2",
            "current_image_tag": "oxy:v1",
            "held_until": None,
        }
    ])
    state = _WatchdogState(
        pending_target="oxy:v2",
        rollback_to="oxy:v1",
        failure_count=update_agent._WATCHDOG_FAILURE_THRESHOLD - 1,
    )
    applied: list[str] = []
    with patch.object(update_agent, "apply_update", lambda cfg, tag: applied.append(tag) or True):
        _run(_tick(client, _cfg(), state))

    assert applied == ["oxy:v1"], "should re-apply the baseline tag"
    assert state.pending_target is None, "watchdog should clear after rollback"
    assert state.failure_count == 0
    # One report POST: last_update_result=reverted.
    assert len(client.posts) == 1
    assert client.posts[0]["json"] == {"last_update_result": "reverted"}


def test_watchdog_rollback_without_baseline_still_reports():
    """If the watchdog trips before we ever had a known-good tag
    to revert to, we still report `reverted` so the operator
    sees the failure — they intervene by hand."""
    client = _FakeClient([
        {
            "target_image_tag": "oxy:v2",
            "current_image_tag": None,
            "held_until": None,
        }
    ])
    state = _WatchdogState(
        pending_target="oxy:v2",
        rollback_to=None,
        failure_count=update_agent._WATCHDOG_FAILURE_THRESHOLD - 1,
    )
    applied: list[str] = []
    with patch.object(update_agent, "apply_update", lambda cfg, tag: applied.append(tag) or True):
        _run(_tick(client, _cfg(), state))

    assert applied == [], "no baseline means no rollback apply"
    assert client.posts[0]["json"] == {"last_update_result": "reverted"}
    assert state.pending_target is None
