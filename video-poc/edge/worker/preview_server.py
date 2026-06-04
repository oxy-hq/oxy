"""HTTP server that exposes the most recent decoded frame per camera
as JPEG, for the Oxy UI's "thumbnail in the camera list" feature.

## Routes

  GET  /preview/cameras/{camera_id}/snapshot.jpg
       → 200 image/jpeg  — encoded from CameraReader.latest_jpeg()
       → 404             — unknown camera_id
       → 503             — camera exists but hasn't decoded a frame yet
                           (just started / reconnecting)
  GET  /preview/health
       → 200 application/json — { "ok": true, "cameras": N }

## Auth posture

NONE at the HTTP layer. The edge box's network boundary IS the auth
boundary: in production this server binds only on the Tailscale
interface, and Tailscale ACLs restrict who can reach it (Oxy's
identity). In dev (docker-compose) it binds 0.0.0.0:8090 and we
rely on the operator not exposing that port publicly.

We resisted adding a shared-secret header for now: it adds config
surface, doesn't actually defend against on-LAN attackers, and the
real auth story is Tailscale. Revisit if customers ask.

## Frame freshness

The CameraReader writes `self._latest_frame` on every successful
decode (every frame, ~25fps). The HTTP handler reads under the same
lock and JPEG-encodes on demand. Worst-case staleness equals
(1 / camera fps) + encode time — well under 100ms in practice.
"""
from __future__ import annotations

import json
from typing import Mapping

from aiohttp import web

from .camera import CameraReader
from .log import log


def build_app(readers: Mapping[str, CameraReader]) -> web.Application:
    """Build the aiohttp application around a (mutable, externally-owned)
    map of camera_id → CameraReader. Caller passes the same map that
    `main.run` populates so reconfigurations land automatically.
    """
    app = web.Application()

    async def snapshot(request: web.Request) -> web.Response:
        cam_id = request.match_info["camera_id"]
        reader = readers.get(cam_id)
        if reader is None:
            return web.Response(status=404, text=f"unknown camera: {cam_id}")
        jpeg = reader.latest_jpeg()
        if jpeg is None:
            return web.Response(
                status=503,
                text="no frame yet (camera starting up or reconnecting)",
            )
        return web.Response(
            body=jpeg,
            content_type="image/jpeg",
            headers={
                # Short cache so the Oxy proxy and the browser keep the
                # snapshot moving. 1s is enough to coalesce duplicate
                # in-flight requests without making the preview stale.
                "Cache-Control": "no-store",
            },
        )

    async def health(_request: web.Request) -> web.Response:
        return web.Response(
            body=json.dumps({"ok": True, "cameras": len(readers)}),
            content_type="application/json",
        )

    app.router.add_get("/preview/cameras/{camera_id}/snapshot.jpg", snapshot)
    app.router.add_get("/preview/health", health)
    return app


async def run_server(
    readers: Mapping[str, CameraReader],
    host: str = "0.0.0.0",
    port: int = 8090,
) -> None:
    """Spin up the aiohttp server alongside the worker's main asyncio
    loop. The Oxy backend proxies `GET /preview/...` from here over
    Tailscale (or whatever reaches the edge box) — see the snapshot
    proxy in `crates/cameras/src/service/preview.rs`.
    """
    app = build_app(readers)
    runner = web.AppRunner(app)
    await runner.setup()
    site = web.TCPSite(runner, host=host, port=port)
    await site.start()
    log("info", "preview.server_started", host=host, port=port)
    # Don't return — keep the runner alive as long as the parent task
    # is running. `asyncio.gather` in main.py treats this as one of
    # the long-lived tasks; cancelling the task tears the server down.
    try:
        # Sleep forever; awaiting the runner doesn't exist as an API.
        # The site stays up because runner lives in our closure.
        import asyncio

        while True:
            await asyncio.sleep(3600)
    finally:
        await runner.cleanup()
        log("info", "preview.server_stopped")
