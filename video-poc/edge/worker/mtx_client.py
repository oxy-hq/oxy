"""Async client for the MediaMTX HTTP control API (v3).

MediaMTX exposes a small REST surface on `apiAddress` (default :9997)
for runtime path config. We use it from the edge worker to keep MTX's
paths in sync with the camera list Oxy returns from
`/control/boxes/{box_id}/config`. No restart, hot reconfig.

## Auth posture

The MTX API has no auth by default. We listen only on the loopback +
the compose internal network in the PoC, and in production both the
worker and MTX run on the same edge box (so the API binding is the
edge box itself). Tailscale only exposes the HLS port (8888), never
the API port (9997).

If we ever need to expose the API across machines, MediaMTX supports
`api: yes` with `webrtc` or `authMethod` schemes — but the simpler
answer is to keep the API local-only.

## Endpoints we use

  GET    /v3/config/paths/list       → existing paths
  POST   /v3/config/paths/add/{name} → register a new path
  DELETE /v3/config/paths/delete/{name}

There's also `/patch/{name}` for in-place updates; we treat patching
as "delete + add" so the reconciler stays single-shaped.
"""
from __future__ import annotations

from typing import Any

import aiohttp

from .log import log


class MtxApiError(RuntimeError):
    """The MTX API returned a non-success status or was unreachable."""


class MtxClient:
    """Thin async wrapper. One client per worker process; the session is
    reused across requests so connection setup amortizes.
    """

    def __init__(self, base_url: str, timeout_s: float = 5.0) -> None:
        self.base_url = base_url.rstrip("/")
        self._timeout = aiohttp.ClientTimeout(total=timeout_s)
        self._session: aiohttp.ClientSession | None = None

    async def __aenter__(self) -> "MtxClient":
        self._session = aiohttp.ClientSession(timeout=self._timeout)
        return self

    async def __aexit__(self, *_exc: Any) -> None:
        if self._session is not None:
            await self._session.close()
            self._session = None

    def _session_or_raise(self) -> aiohttp.ClientSession:
        if self._session is None:
            raise MtxApiError(
                "MtxClient used outside `async with` — session not initialised"
            )
        return self._session

    async def list_paths(self) -> list[dict[str, Any]]:
        """Return every path MTX currently knows about. Used by the
        sync reconciler to compute (add, keep, remove) sets.
        """
        url = f"{self.base_url}/v3/config/paths/list"
        try:
            async with self._session_or_raise().get(url) as r:
                if r.status != 200:
                    raise MtxApiError(f"GET {url} → {r.status}")
                body = await r.json()
        except aiohttp.ClientError as e:
            raise MtxApiError(f"GET {url} failed: {e}") from e
        # MTX v3 wraps the list in {"itemCount": N, "pageCount": M, "items": [...]}
        items = body.get("items") if isinstance(body, dict) else body
        return list(items or [])

    async def list_path_stats(self) -> list[dict[str, Any]]:
        """Runtime stats per path: `bytesSent`, `bytesReceived`,
        `ready`, `online`, etc. Distinct from `list_paths` which
        returns config-only.

        Used by the bandwidth reporter to track per-path traffic so
        we can see who's eating the Tailscale Funnel monthly quota.
        """
        url = f"{self.base_url}/v3/paths/list"
        try:
            async with self._session_or_raise().get(url) as r:
                if r.status != 200:
                    raise MtxApiError(f"GET {url} → {r.status}")
                body = await r.json()
        except aiohttp.ClientError as e:
            raise MtxApiError(f"GET {url} failed: {e}") from e
        items = body.get("items") if isinstance(body, dict) else body
        return list(items or [])

    async def add_path(self, name: str, config: dict[str, Any]) -> None:
        """Register a new path with the given config (source URL etc).
        MTX validates the body — invalid fields surface as 400.
        """
        url = f"{self.base_url}/v3/config/paths/add/{name}"
        try:
            async with self._session_or_raise().post(url, json=config) as r:
                if r.status >= 300:
                    body = await r.text()
                    raise MtxApiError(f"POST {url} → {r.status}: {body[:200]}")
        except aiohttp.ClientError as e:
            raise MtxApiError(f"POST {url} failed: {e}") from e
        log("info", "mtx.path_added", name=name)

    async def delete_path(self, name: str) -> None:
        """Remove a path. Idempotent — a 404 is treated as "already
        gone" so the reconciler can be naive about state.
        """
        url = f"{self.base_url}/v3/config/paths/delete/{name}"
        try:
            async with self._session_or_raise().delete(url) as r:
                if r.status == 404:
                    return  # already gone
                if r.status >= 300:
                    body = await r.text()
                    raise MtxApiError(f"DELETE {url} → {r.status}: {body[:200]}")
        except aiohttp.ClientError as e:
            raise MtxApiError(f"DELETE {url} failed: {e}") from e
        log("info", "mtx.path_removed", name=name)
