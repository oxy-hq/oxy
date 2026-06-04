"""SQLite-backed outbox.

Atomicity contract: enqueue() commits before returning. drain_loop() never
deletes or marks rows as synced until the control API returns 2xx. On any
non-2xx response or network failure we back off exponentially and retry. This
gives us the exactly-once semantics described in the architecture doc, paired
with the API's ON CONFLICT DO NOTHING on event_id / report_id.

Two payload kinds share one queue:
  'event'              -> POST /control/events             (camera_events)
  'compliance_report'  -> POST /control/compliance-reports (camera_compliance_reports)
"""
from __future__ import annotations

import asyncio
import json
import random
import sqlite3
import time
from contextlib import contextmanager
from pathlib import Path
from typing import Any, Iterator

import httpx

from .log import log

_BATCH = 100
_BACKOFF_INITIAL_S = 1.0
_BACKOFF_MAX_S = 60.0

KIND_EVENT = "event"
KIND_COMPLIANCE = "compliance_report"

_KIND_ENDPOINTS = {
    KIND_EVENT: "/control/events",
    KIND_COMPLIANCE: "/control/compliance-reports",
}

# Wire shape per kind — the Rust route expects an object-wrapped batch
# (`{"events": [...]}` / `{"reports": [...]}`), not a bare array.
_KIND_BODY_KEY = {
    KIND_EVENT: "events",
    KIND_COMPLIANCE: "reports",
}


class Outbox:
    def __init__(self, path: str) -> None:
        Path(path).parent.mkdir(parents=True, exist_ok=True)
        self._path = path
        with self._conn() as c:
            c.execute("PRAGMA journal_mode = WAL")
            c.execute("PRAGMA synchronous = NORMAL")
            c.execute(
                """
                CREATE TABLE IF NOT EXISTS events_outbox (
                    rowid       INTEGER PRIMARY KEY AUTOINCREMENT,
                    event_id    TEXT NOT NULL UNIQUE,
                    payload     TEXT NOT NULL,
                    enqueued_at REAL NOT NULL,
                    synced_at   REAL
                )
                """
            )
            c.execute(
                "CREATE INDEX IF NOT EXISTS outbox_pending_idx "
                "ON events_outbox (synced_at) WHERE synced_at IS NULL"
            )
            # Backward-compat migration: add 'kind' column if it's missing.
            cols = {row[1] for row in c.execute("PRAGMA table_info(events_outbox)").fetchall()}
            if "kind" not in cols:
                c.execute(
                    f"ALTER TABLE events_outbox ADD COLUMN kind TEXT NOT NULL DEFAULT '{KIND_EVENT}'"
                )

    @contextmanager
    def _conn(self) -> Iterator[sqlite3.Connection]:
        c = sqlite3.connect(self._path, isolation_level=None, timeout=30.0)
        try:
            yield c
        finally:
            c.close()

    # ------------------------------------------------------------------
    # Enqueue
    # ------------------------------------------------------------------

    def enqueue(self, event: dict[str, Any]) -> None:
        """Enqueue a camera_event payload (the original path)."""
        self._enqueue(event["event_id"], event, KIND_EVENT)

    def enqueue_compliance_report(self, report: dict[str, Any]) -> None:
        """Enqueue a camera_compliance_reports payload."""
        self._enqueue(report["report_id"], report, KIND_COMPLIANCE)

    def _enqueue(self, unique_id: str, payload: dict[str, Any], kind: str) -> None:
        with self._conn() as c:
            c.execute(
                "INSERT OR IGNORE INTO events_outbox (event_id, payload, enqueued_at, kind) "
                "VALUES (?, ?, ?, ?)",
                (unique_id, json.dumps(payload, default=str), time.time(), kind),
            )

    # ------------------------------------------------------------------
    # Inspect
    # ------------------------------------------------------------------

    def pending_count(self) -> int:
        with self._conn() as c:
            row = c.execute("SELECT COUNT(*) FROM events_outbox WHERE synced_at IS NULL").fetchone()
            return int(row[0])

    # ------------------------------------------------------------------
    # Drain
    # ------------------------------------------------------------------

    def _take_batch(self, kind: str) -> list[tuple[int, dict[str, Any]]]:
        with self._conn() as c:
            rows = c.execute(
                "SELECT rowid, payload FROM events_outbox "
                "WHERE synced_at IS NULL AND kind = ? "
                "ORDER BY rowid LIMIT ?",
                (kind, _BATCH),
            ).fetchall()
        return [(r[0], json.loads(r[1])) for r in rows]

    def _mark_synced(self, rowids: list[int]) -> None:
        if not rowids:
            return
        now = time.time()
        with self._conn() as c:
            c.executemany(
                "UPDATE events_outbox SET synced_at = ? WHERE rowid = ?",
                [(now, rid) for rid in rowids],
            )

    async def drain_loop(self, oxy_url: str, auth: httpx.Auth) -> None:
        """One drain loop that handles both kinds. Sequential per kind per
        tick; idle tick is 2s. Backoff is per-kind to avoid one bad
        endpoint starving the other.

        Uses its own httpx.AsyncClient (separate from the main worker's)
        so the drain loop's lifecycle is independent. `auth` is an
        httpx.Auth instance that stamps `Authorization` on every
        request — wired by the caller as either `BearerAuth(token)`
        (legacy) or `DeviceJwtAuth(minter)` (IoT Phase 3+).
        """
        backoffs: dict[str, float] = {k: _BACKOFF_INITIAL_S for k in _KIND_ENDPOINTS}
        async with httpx.AsyncClient(base_url=oxy_url, timeout=30.0, auth=auth) as client:
            while True:
                idle = True
                for kind, path in _KIND_ENDPOINTS.items():
                    batch = self._take_batch(kind)
                    if not batch:
                        backoffs[kind] = _BACKOFF_INITIAL_S
                        continue
                    idle = False
                    rowids = [r for r, _ in batch]
                    payload = {_KIND_BODY_KEY[kind]: [p for _, p in batch]}
                    try:
                        r = await client.post(path, json=payload)
                        r.raise_for_status()
                        self._mark_synced(rowids)
                        log("info", "outbox.drained", kind=kind, count=len(rowids))
                        backoffs[kind] = _BACKOFF_INITIAL_S
                    except Exception as e:
                        log(
                            "warn", "outbox.drain_failed",
                            kind=kind, count=len(rowids), error=str(e),
                            backoff_s=backoffs[kind],
                        )
                        await asyncio.sleep(backoffs[kind] + random.uniform(0, backoffs[kind] * 0.25))
                        backoffs[kind] = min(backoffs[kind] * 2, _BACKOFF_MAX_S)
                if idle:
                    await asyncio.sleep(2.0)
