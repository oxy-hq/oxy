"""Best-effort log shipper for the edge worker (IoT Phase 4b).

Buffers structured log records in memory and POSTs them to
`POST /control/boxes/{id}/logs` in batches. The shipper is
deliberately separate from the outbox:

  * The outbox is the durable, exactly-once path for *events* and
    *compliance reports* — losing one would be a customer-visible
    bug. SQLite + per-row tracking is worth the overhead there.
  * Logs are inherently lossy. A box that's offline for 10 minutes
    should resume shipping fresh lines, not flood the operator
    with 10 minutes of stale boot-up noise. An in-memory ring
    buffer with a small backlog is the right shape.

If the cap is hit (~2000 lines) the *oldest* lines get dropped —
the most recent activity is what an operator opening the viewer
needs. The drop count is logged via a `log_shipper.dropped` event
so operators can see when they were on the wrong side of a flood.
"""
from __future__ import annotations

import asyncio
import collections
import threading
from typing import Any

import httpx


# Tuned for ~50 boxes × 100 lines/min. Each entry is small (~300
# bytes typical), so 2k bounds memory at <1 MB per worker.
_MAX_BUFFER = 2000
# Flush triggers — whichever fires first wins.
_FLUSH_INTERVAL_S = 30.0
_FLUSH_BATCH_THRESHOLD = 50
# Drain ceiling per POST. Larger than the trigger so we catch up
# after a stall without unbounded request size.
_MAX_BATCH_PER_POST = 500

# Reserved event-name prefixes the shipper itself emits. Skipping
# them in enqueue() prevents a self-feedback loop: if the
# control-plane is unreachable, our retry warnings would otherwise
# pile into the same buffer they're trying to drain.
_SUPPRESS_EVENT_PREFIX = "log_shipper."


class LogShipper:
    """In-memory bounded log buffer + drain coroutine."""

    def __init__(self, max_buffer: int = _MAX_BUFFER) -> None:
        self._buf: collections.deque[dict[str, Any]] = collections.deque(maxlen=max_buffer)
        self._lock = threading.Lock()
        self._dropped_since_last_flush = 0

    # ------------------------------------------------------------------
    # Producer side — called from sync `log()` everywhere in the worker.
    # ------------------------------------------------------------------

    def enqueue(self, line: dict[str, Any]) -> None:
        """Push one record. Drops the oldest entry on overflow.

        Records emitted by the shipper itself (`log_shipper.*`) are
        skipped so a control-plane outage doesn't spiral.
        """
        event = line.get("event", "")
        if isinstance(event, str) and event.startswith(_SUPPRESS_EVENT_PREFIX):
            return
        with self._lock:
            if len(self._buf) >= self._buf.maxlen:
                self._dropped_since_last_flush += 1
            self._buf.append(line)

    def _drain(self, max_lines: int) -> tuple[list[dict[str, Any]], int]:
        with self._lock:
            out: list[dict[str, Any]] = []
            while self._buf and len(out) < max_lines:
                out.append(self._buf.popleft())
            dropped = self._dropped_since_last_flush
            self._dropped_since_last_flush = 0
            return out, dropped

    def _requeue_front(self, batch: list[dict[str, Any]]) -> None:
        """On a failed POST, push lines back to the front of the
        buffer so order is preserved-ish. If the buffer was filled
        by other producers in the meantime, the oldest of the
        re-pushed lines may be dropped — that's fine, we already
        treat logs as lossy.
        """
        with self._lock:
            for line in reversed(batch):
                if len(self._buf) >= self._buf.maxlen:
                    self._dropped_since_last_flush += 1
                    self._buf.popleft()
                self._buf.appendleft(line)

    def pending_count(self) -> int:
        with self._lock:
            return len(self._buf)

    # ------------------------------------------------------------------
    # Consumer side — one long-lived asyncio task in run().
    # ------------------------------------------------------------------

    async def run(self, client: httpx.AsyncClient, box_id: str) -> None:
        """Drain loop. Flushes every `_FLUSH_INTERVAL_S` or once the
        buffer reaches `_FLUSH_BATCH_THRESHOLD`, whichever comes
        first. Uses the caller-supplied `httpx.AsyncClient` so it
        inherits the bearer + Tailscale-IP headers without
        re-wiring auth.
        """
        # Local import to keep the module import-light; `log()` is
        # only needed for the shipper's own status events.
        from .log import log

        path = f"/control/boxes/{box_id}/logs"
        backoff = 1.0
        while True:
            await self._wait_for_flush_trigger()
            batch, dropped = self._drain(_MAX_BATCH_PER_POST)
            if dropped:
                # Recording on the wire — the operator sees these
                # in the same viewer as the other lines, with a
                # `dropped_count` field. The synthetic `ts` is now
                # so it sorts at the end of the batch it represents.
                batch.append(
                    {
                        "ts": _now_iso(),
                        "severity": "warn",
                        "event": "log_shipper.dropped",
                        "fields_json": f'{{"dropped_count": {dropped}}}',
                    }
                )
            if not batch:
                continue
            try:
                r = await client.post(path, json={"logs": batch})
                r.raise_for_status()
                backoff = 1.0
            except Exception as e:
                # Log to stdout only — `log_shipper.*` is suppressed
                # from re-enqueueing so this never recurses.
                log(
                    "warn",
                    "log_shipper.post_failed",
                    count=len(batch),
                    error=str(e),
                    backoff_s=backoff,
                )
                self._requeue_front(batch)
                await asyncio.sleep(backoff)
                backoff = min(backoff * 2, 60.0)

    async def _wait_for_flush_trigger(self) -> None:
        """Block until either the interval elapses or the buffer
        crosses the batch threshold. Polling at 1 s gives us
        responsive batch-trigger flushes without burning CPU.
        """
        waited = 0.0
        while waited < _FLUSH_INTERVAL_S:
            if self.pending_count() >= _FLUSH_BATCH_THRESHOLD:
                return
            await asyncio.sleep(1.0)
            waited += 1.0


def _now_iso() -> str:
    """RFC3339 with explicit `Z` — matches what the Rust DTO's
    `chrono::DateTime<Utc>` expects from the wire.
    """
    import time

    return time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
