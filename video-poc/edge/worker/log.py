"""Structured JSON logging — one line per event, grep-friendly.

When a `LogShipper` is attached at worker boot, each emitted line
also gets pushed onto the shipper's in-memory buffer for batched
upload to the Oxy control plane. Stdout still gets the same JSON
line so container-level log scraping (`docker logs`) keeps
working unchanged.
"""
from __future__ import annotations

import json
import os
import sys
import time
from typing import Any


_LEVELS = {"debug": 10, "info": 20, "warn": 30, "warning": 30, "error": 40}
_LEVEL = _LEVELS.get(os.environ.get("LOG_LEVEL", "info").lower(), 20)

# Optional shipper handle. Set via `attach_shipper()` from main()
# after construction. None = stdout-only (e.g. unit tests, or a
# deployment with `LOG_SHIPPER=off`).
_SHIPPER: Any = None


def attach_shipper(shipper: Any) -> None:
    """Wire a `LogShipper` so every emitted line also gets queued
    for control-plane upload. Idempotent re-attach is fine — the
    last one wins.
    """
    global _SHIPPER
    _SHIPPER = shipper


def log(level: str, msg: str, **fields: Any) -> None:
    if _LEVELS.get(level, 20) < _LEVEL:
        return
    # The original `ts` (no `Z`) is preserved for stdout so
    # human-eyeballing the container logs reads the same as before.
    ts = time.strftime("%Y-%m-%dT%H:%M:%S", time.gmtime())
    rec: dict[str, Any] = {
        "ts": ts,
        "level": level,
        "msg": msg,
        **fields,
    }
    sys.stdout.write(json.dumps(rec, default=str) + "\n")
    sys.stdout.flush()

    if _SHIPPER is not None:
        # Match the Rust DTO's wire shape: ts (RFC3339 with `Z`),
        # severity, event, fields_json (pre-encoded object). The
        # shipper's enqueue is sync + thread-safe so calling from
        # any context (sync helper, async coroutine, background
        # OpenCV thread) is fine.
        try:
            _SHIPPER.enqueue(
                {
                    "ts": ts + "Z",
                    "severity": level,
                    "event": msg,
                    "fields_json": json.dumps(fields, default=str),
                }
            )
        except Exception:
            # Never let logging take the worker down. Stdout
            # already has the line; the shipper is best-effort.
            pass
