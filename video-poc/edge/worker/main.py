"""Edge worker entrypoint.

Lifecycle:
  1. Read edge_box_id from env (provision.py wrote it after the
     announce → bootstrap dance against the control plane).
  2. Load the device identity from disk and build the JWT-minting
     auth flow; every /control/* call signs a fresh JWT.
  3. Fetch camera config from Oxy via GET /control/boxes/{id}/config.
  4. Spawn one CameraReader per camera (threads, since OpenCV is
     blocking).
  5. Spawn outbox drain task.
  6. Spawn box + per-camera health tasks.
  7. Spawn config poller (re-sync every CONFIG_POLL_INTERVAL_S).

Failure posture:
  - Config fetch retries with backoff on transient errors.
  - RTSP reconnects are handled inside CameraReader.
  - Outbox never drops unsynced rows.

Required env:
  - OXY_URL              base URL of the Oxy server (e.g. https://api.oxy.tech)
  - EDGE_BOX_ID          UUID of this edge box, written to edge.env by
                         worker.provision after a successful bootstrap.

Optional env:
  - OUTBOX_PATH          default: /var/lib/outbox/<edge_box_id>/outbox.sqlite
  - HEALTH_INTERVAL_S    default: 30
  - CONFIG_POLL_INTERVAL_S  default: 30
  - IMAGE_TAG            default: 'dev' — reported in box_health
  - ANTHROPIC_API_KEY    enables on-trigger Haiku VLM compliance checks
  - UPSELL_CAMERAS       comma-separated camera-name substrings whose
                         (near-field-mic) audio is run through the upsell-
                         detection pipeline. Needs ANTHROPIC_API_KEY. Unset
                         ⇒ audio upsell detection off.
  - EDGE_ROLE            full (cameras + audio, default) | video (cameras only)
                         | audio (upsell audio only — a dedicated low-compute
                         box: no YOLO / MediaMTX / preview).
"""
from __future__ import annotations

import asyncio
import os
from typing import Callable
from uuid import UUID

import anthropic
import httpx

from . import mtx_sync, prompts, upsell
from .camera import CameraReader
from .config import CameraConfig, EdgeConfig
from .health import audio_health_loop, box_health_loop, camera_health_loop
from .identity import DeviceIdentity
from .jwt_mint import DeviceJwtAuth, JwtMinter
from .log import attach_shipper, log
from .log_shipper import LogShipper
from .mtx_client import MtxClient
from .outbox import KIND_COMPLIANCE, KIND_EVENT, Outbox
from .preview_server import run_server as run_preview_server


class StartupError(RuntimeError):
    """Misconfiguration that prevents the worker from booting at all."""


class _NullAsyncContext:
    """Drop-in replacement for `async with MtxClient(...)` when MTX is
    disabled. Lets us keep a single `async with (..., _maybe_mtx_client)`
    site at the top of `run()` instead of branching on `if mtx_api_url`
    in five places.
    """

    async def __aenter__(self) -> None:
        return None

    async def __aexit__(self, *_exc: object) -> None:
        return None


def _maybe_mtx_client(url: str):
    """Build an `MtxClient` if a URL is configured; otherwise return a
    no-op async context. Callers see `mtx: MtxClient | None` and guard
    on `if mtx is not None`.
    """
    if not url:
        return _NullAsyncContext()
    return MtxClient(url)


def _required_env(name: str) -> str:
    v = os.environ.get(name, "").strip()
    if not v:
        raise StartupError(
            f"required env {name} is not set. The operator gets this from "
            "Oxy's POST /{workspace_id}/cameras/edge-boxes and hands it to the installer."
        )
    return v


async def _fetch_config(client: httpx.AsyncClient, box_id: UUID) -> EdgeConfig:
    """Fetch the full edge config (cameras + active domain pack +
    workspace_id).

    Installs the active pack into the prompts module as a side
    effect so subsequent VLM calls render against it. Returns the
    full EdgeConfig — callers pick out what they need
    (`.cameras` for the reader hot-swap path; `.workspace_id`
    for the clip-archive path).
    """
    backoff = 1.0
    while True:
        try:
            r = await client.get(f"/control/boxes/{box_id}/config")
            r.raise_for_status()
            try:
                payload = r.json()
            except ValueError as decode_err:
                # 2xx response with a non-JSON body. The classic cause is an
                # OXY_URL that doesn't include `/api` — the API tree is
                # mounted under /api and the SPA fallback returns index.html
                # (status 200, content-type text/html) for any other path,
                # so r.json() blows up with the opaque "Expecting value:
                # line 1 column 1" message. Surface the actual status,
                # content-type, and a body snippet so the operator can fix
                # OXY_URL without source-diving.
                content_type = r.headers.get("content-type", "")
                body_snippet = r.text[:120].replace("\n", " ").strip()
                hint = (
                    " — looks like HTML; OXY_URL likely needs `/api` appended"
                    if "html" in content_type.lower() or body_snippet.startswith("<")
                    else ""
                )
                raise ValueError(
                    f"non-JSON response from /control/boxes/{box_id}/config "
                    f"(status={r.status_code}, content_type={content_type!r}, "
                    f"body={body_snippet!r}){hint}"
                ) from decode_err
            # Tolerate the pre-Phase-2 array response shape during
            # rollout. New shape: {cameras: [...], domain_pack: {...}}.
            # Old shape: [...] (list of cameras).
            if isinstance(payload, list):
                edge = EdgeConfig(cameras=[CameraConfig.model_validate(c) for c in payload])
            else:
                edge = EdgeConfig.model_validate(payload)
            prompts.set_active_pack(edge.domain_pack)
            return edge
        except Exception as e:
            log("warn", "config.fetch_failed", error=str(e), backoff_s=backoff)
            await asyncio.sleep(backoff)
            backoff = min(backoff * 2, 30.0)


async def _outbox_producer(queue: asyncio.Queue, outbox: Outbox) -> None:
    """Route queued payloads to the right outbox enqueue method by kind."""
    while True:
        item = await queue.get()
        kind = item.get("kind", KIND_EVENT)
        payload = item.get("payload", item)
        if kind == KIND_COMPLIANCE:
            outbox.enqueue_compliance_report(payload)
        else:
            outbox.enqueue(payload)


def _cam_signature(cam: CameraConfig) -> tuple:
    """Comparable fingerprint of a camera's reader-relevant config.

    A change in any of these fields means the corresponding
    `CameraReader` thread must be restarted to pick up the new
    behavior (new RTSP target, new zone polygons, new line counters).
    Name / model are deliberately excluded — they don't influence
    the reader's runtime.

    `zones_json` and `lines_json` are lists of dicts, which aren't
    hashable directly; we fall back to a sorted JSON-repr-ish
    representation. The exact serialization doesn't matter — only
    that equal configs map to equal tuples.
    """
    import json
    return (
        cam.rtsp_url or "",
        cam.substream_url or "",
        json.dumps(cam.zones_json, sort_keys=True, default=str),
        json.dumps(cam.lines_json, sort_keys=True, default=str),
        # Role drives prompt selection in VLM calls. A change should
        # restart the reader so its cached `self.cfg.role` is current
        # without needing a mutate-in-place path.
        cam.role or "",
    )


async def _config_poller(
    client: httpx.AsyncClient,
    box_id: UUID,
    interval_s: int,
    current: dict[str, CameraConfig],
    on_change: Callable[[list[CameraConfig]], None],
    on_every_poll: Callable[[list[CameraConfig]], None],
) -> None:
    """Two callbacks:

    - `on_change` fires only when the camera set itself changes
      (added, removed, or zone/rtsp content change). Owns the
      reader hot-swap path — expensive, so we don't want to fire
      it on every tick.

    - `on_every_poll` fires every successful poll regardless of
      whether anything changed. Owns the MTX path reconcile, which
      MUST run unconditionally because MTX is stateful and may
      have been restarted out from under us — leaving the worker
      thinking "I synced N paths" while MTX has an empty path
      list. Reconcile is idempotent, so the overhead at steady
      state is one cheap GET to MTX's API per poll cycle.
    """
    while True:
        await asyncio.sleep(interval_s)
        try:
            new_edge = await _fetch_config(client, box_id)
            new_list = new_edge.cameras
            on_every_poll(new_list)

            new_map = {str(c.id): c for c in new_list}
            added = new_map.keys() - current.keys()
            removed = current.keys() - new_map.keys()
            # Detect content-only changes (same id, different zones /
            # rtsp_url / etc.). Reader-side hot-reload depends on this
            # — without it, a zone polygon edit in Oxy would never
            # take effect without a worker restart.
            changed = {
                cid for cid in new_map.keys() & current.keys()
                if _cam_signature(new_map[cid]) != _cam_signature(current[cid])
            }
            if added or removed or changed:
                log("info", "config.changed",
                    added=list(added),
                    removed=list(removed),
                    changed=list(changed))
                on_change(new_list)
                current.clear()
                current.update(new_map)
        except Exception as e:
            log("warn", "config.poll_failed", error=str(e))


async def _bandwidth_reporter(
    mtx: MtxClient,
    client: httpx.AsyncClient,
    box_id: UUID,
    funnel_hostname: str,
    interval_s: int = 300,
) -> None:
    """Log per-path byte deltas from MTX every `interval_s`, AND
    POST the rollup to Oxy so the cameras dashboard can surface
    per-box outbound bytes.

    The point is rough visibility into Tailscale Funnel quota
    consumption: WebRTC media + TURN-over-TLS reads through Funnel
    are billed against a 100 GB/month tailnet-wide free tier, and
    a single chatty workspace can consume the whole budget without
    anyone noticing until live preview just stops working.

    What we measure here is MTX's `bytesSent` per path, summed —
    which counts ALL outbound from MTX, not just the Funnel-
    attributable subset. Intentional overcount: easier to
    instrument than splitting by destination, and "we're
    approaching the cap" is a safe-side error. For an exact
    per-tailnet figure, hit Tailscale's Admin API or the admin
    console at https://login.tailscale.com/admin/usage.
    """
    prev: dict[str, int] = {}
    while True:
        await asyncio.sleep(interval_s)
        try:
            paths = await mtx.list_path_stats()
            cur = {p.get("name"): int(p.get("bytesSent", 0)) for p in paths if p.get("name")}
            # Sum positive deltas; a path being recreated resets
            # the counter so negative deltas are dropped.
            delta_bytes = sum(max(0, cur.get(k, 0) - prev.get(k, 0)) for k in cur)
            # POST to Oxy so it can stamp `edge_boxes.bandwidth_5min_bytes`.
            # Best-effort: a failed POST shouldn't kill the loop —
            # we'd rather lose one sample than break visibility
            # for the whole reporter.
            try:
                resp = await client.post(
                    f"/control/boxes/{box_id}/bandwidth",
                    json={"delta_bytes": delta_bytes, "interval_s": interval_s},
                )
                resp.raise_for_status()
            except Exception as post_err:
                log("warn", "bandwidth.post_failed", error=str(post_err))
            log(
                "info",
                "bandwidth.delta",
                funnel_hostname=funnel_hostname or "(unset)",
                interval_s=interval_s,
                delta_bytes=delta_bytes,
                paths_count=len(cur),
            )
            prev = cur
        except Exception as e:
            log("warn", "bandwidth.report_failed", error=str(e))


async def run() -> None:
    oxy_url = _required_env("OXY_URL")
    edge_box_id_str = _required_env("EDGE_BOX_ID")
    box_id = UUID(edge_box_id_str)

    # Per-replica outbox path so `docker compose --scale edge=N` doesn't
    # race multiple workers against one SQLite file on the shared volume.
    default_outbox = f"/var/lib/outbox/{box_id}/outbox.sqlite"
    # `or default_outbox` — not `os.environ.get(..., default_outbox)`.
    # The compose declares `OUTBOX_PATH: ${OUTBOX_PATH:-}` so the env
    # var is always present, just often empty. `get(..., fallback)`
    # only uses the fallback when the key is missing; passing an
    # empty string skips it and we'd open SQLite at path "", which
    # silently creates a fresh temp DB per connection. That looks
    # like an outbox table that's missing every call.
    outbox_path = os.environ.get("OUTBOX_PATH") or default_outbox
    health_interval = int(os.environ.get("HEALTH_INTERVAL_S", "30"))
    config_interval = int(os.environ.get("CONFIG_POLL_INTERVAL_S", "30"))
    image_tag = os.environ.get("IMAGE_TAG", "dev")
    # EDGE_ROLE selects the workload: 'full' (cameras + audio, default), 'video'
    # (cameras only, no upsell audio), or 'audio' (upsell audio only — a
    # dedicated low-compute box that pulls register-mic audio and skips all
    # YOLO / MediaMTX / preview / clips).
    edge_role = os.environ.get("EDGE_ROLE", "full").strip().lower()
    run_video = edge_role in ("full", "video")
    run_audio = edge_role in ("full", "audio")
    log("info", "edge.role", role=edge_role, video=run_video, audio=run_audio)

    log("info", "edge.boot",
        edge_box_id=str(box_id),
        oxy_url=oxy_url,
        outbox=outbox_path)

    outbox = Outbox(outbox_path)
    queue: asyncio.Queue = asyncio.Queue(maxsize=10_000)
    loop = asyncio.get_running_loop()

    # In-memory log shipper — pushes structured `log()` calls to
    # the control plane in batches. Attaches before any further
    # `log()` calls so boot-time lines also end up in the operator
    # viewer. Setting LOG_SHIPPER=off makes this a no-op (useful
    # when debugging a box that can't reach Oxy yet).
    log_shipper: LogShipper | None = None
    if os.environ.get("LOG_SHIPPER", "on").lower() not in {"off", "0", "false"}:
        log_shipper = LogShipper()
        attach_shipper(log_shipper)

    # VLM client is optional — if ANTHROPIC_API_KEY is unset, compliance
    # reports are simply disabled. Events still flow as before.
    vlm_client: anthropic.AsyncAnthropic | None = None
    if os.environ.get("ANTHROPIC_API_KEY"):
        vlm_client = anthropic.AsyncAnthropic()
        log("info", "edge.vlm_enabled",
            model=os.environ.get("VLM_MODEL", "claude-haiku-4-5-20251001"))
    else:
        log("warn", "edge.vlm_disabled",
            reason="ANTHROPIC_API_KEY not set; compliance reports off")

    # Audio upsell detection (Phase 2). Opt-in per box via UPSELL_CAMERAS
    # (comma-separated camera-name substrings). Reuses the VLM's
    # ANTHROPIC_API_KEY for the intent classifier; a separate *sync* client
    # because each AudioReader classifies from its own capture thread. The
    # STT model loads once here (a few seconds) and is shared across readers.
    upsell_allow = upsell.upsell_cameras()
    upsell_stt: upsell.Transcriber | None = None
    upsell_intent_client: anthropic.Anthropic | None = None
    if run_audio and upsell_allow and os.environ.get("ANTHROPIC_API_KEY"):
        upsell_stt = upsell.Transcriber()
        upsell_intent_client = anthropic.Anthropic()
        log("info", "edge.upsell_enabled", cameras=upsell_allow,
            stt_model=os.environ.get("UPSELL_STT_MODEL", "small.en"))
    elif run_audio and upsell_allow:
        log("warn", "edge.upsell_disabled",
            reason="ANTHROPIC_API_KEY not set; upsell intent needs it",
            cameras=upsell_allow)

    # MTX dynamic-path API. Defaults to the compose service hostname;
    # production overrides via env. Empty string disables MTX sync
    # entirely (useful for tests / dev with no MTX in the stack).
    # Audio-only boxes have no video path → no MediaMTX.
    mtx_api_url = (
        os.environ.get("MEDIAMTX_API_URL", "http://mediamtx:9997").strip()
        if run_video else ""
    )

    # Auth: per-device JWT signed with the HMAC secret persisted at
    # factory enroll. We mint a fresh JWT every ~55min and inject it
    # via `DeviceJwtAuth` on every /control/* request. A missing
    # identity file is a hard configuration error — the device can't
    # authenticate without it, so fail loud instead of silently
    # 401-ing forever.
    identity = DeviceIdentity.load()
    if identity is None:
        raise StartupError(
            "no device identity at /var/lib/oxy/device.json — "
            "rerun the 'Add device' install snippet on this box."
        )
    minter = JwtMinter(identity.device_id, identity.device_secret)
    control_auth = DeviceJwtAuth(minter)
    log("info", "edge.auth_mode",
        mode="jwt",
        device_id=str(identity.device_id))

    # Headers carry everything EXCEPT auth — the auth object
    # injects `Authorization` per-request so JWT rotation is
    # transparent to the call sites. The other headers
    # (Tailscale IP, funnel hostname) are static for the
    # process lifetime so they belong in the dict.
    headers: dict[str, str] = {}

    # EDGE_PUBLIC_HOST tells Oxy how to reach this edge back for the
    # preview / HLS proxies. Set this when the box's docker bridge IP
    # (which Oxy would otherwise see in X-Forwarded-For / socket peer)
    # isn't routable from the Oxy side:
    #
    #   - **Dev**, Oxy on host + edge in compose:
    #       EDGE_PUBLIC_HOST=127.0.0.1
    #     Both preview-server :8090 and MediaMTX :8888 are mapped to
    #     host ports, so 127.0.0.1 from Oxy's perspective reaches them.
    #
    #   - **Prod**, Tailscale-native:
    #       EDGE_PUBLIC_HOST=<this box's Tailscale IP, e.g. 100.64.1.42>
    #     Or omit and rely on Oxy reading XFF / socket peer (works iff
    #     Oxy is reachable directly via Tailscale too).
    #
    # When set, the worker stamps `X-Edge-Tailscale-IP: <value>` on
    # every /control/* call. The Oxy middleware records that into
    # `edge_boxes.tailscale_ip`; subsequent preview / HLS proxies use
    # it as the upstream host.
    edge_public_host = os.environ.get("EDGE_PUBLIC_HOST", "").strip()
    if edge_public_host:
        headers["X-Edge-Tailscale-IP"] = edge_public_host
        log("info", "edge.public_host", value=edge_public_host)

    # EDGE_FUNNEL_HOSTNAME — the box's Tailscale Funnel public DNS
    # name (e.g. `video-poc-edge.tail666f9b.ts.net`). Sent on every
    # /control/* call so the Oxy auth middleware can stamp
    # `edge_boxes.funnel_hostname`, which is what the WebRTC session
    # endpoint hands back to operator browsers as the WHEP URL +
    # TURN-over-TLS host. Without it the WebRTC live-preview path
    # is unavailable and the UI auto-falls back to HLS.
    #
    # The operator reads the hostname from `tailscale serve status`
    # on the box after Funnel is enabled, and pastes it into .env.
    # Defaulting to empty is intentional — boxes that never enable
    # Funnel just don't get WebRTC and the rest of the pipeline
    # is unaffected.
    edge_funnel_hostname = os.environ.get("EDGE_FUNNEL_HOSTNAME", "").strip()
    if edge_funnel_hostname:
        headers["X-Edge-Funnel-Hostname"] = edge_funnel_hostname
        log("info", "edge.funnel_hostname", value=edge_funnel_hostname)
    else:
        # Loud "unset" log makes "is the header being sent?" one
        # grep instead of inferring from absence. The previous silent
        # path produced WebRTC-session "edge box has no funnel_hostname
        # yet" failures that took source-diving to diagnose.
        log(
            "warn",
            "edge.funnel_hostname_unset",
            hint=(
                "EDGE_FUNNEL_HOSTNAME is empty — WebRTC live preview will "
                "be unavailable. Check /var/lib/outbox/funnel.env on the "
                "host and that the tailscale-funnel-init sidecar succeeded."
            ),
        )

    async with (
        httpx.AsyncClient(
            base_url=oxy_url,
            timeout=15.0,
            headers=headers,
            auth=control_auth,
        ) as client,
        _maybe_mtx_client(mtx_api_url) as mtx,
    ):
        edge_cfg = await _fetch_config(client, box_id)
        cams = edge_cfg.cameras
        workspace_id = edge_cfg.workspace_id
        log("info", "edge.config", cameras=[c.name for c in cams],
            workspace_id=workspace_id)

        # Cameras with no rtsp_url come from inventory-imported rows
        # where the rtspAlias hasn't been fetched yet (owner key not yet
        # available, or operator hasn't pasted one in). Skip them — they
        # show up again on the next config poll if the URL is filled in.
        skipped = [c.name for c in cams if not c.rtsp_url]
        if skipped:
            log("info", "edge.skip_no_rtsp", cameras=skipped)
        cams = [c for c in cams if c.rtsp_url]

        # Sync MTX paths with the camera list. Best-effort: if MTX
        # isn't reachable the reconciler logs + returns; the next
        # config-poll iteration retries.
        if mtx is not None:
            await mtx_sync.reconcile(mtx, cams)

        readers: dict[str, CameraReader] = {}
        # Audio-upsell readers, keyed by camera id like `readers` — spawned
        # alongside a CameraReader when the camera matches UPSELL_CAMERAS.
        audio_readers: dict[str, upsell.AudioReader] = {}
        # Per-camera health tasks live in their own dict so we can
        # cancel them individually during hot-swap. The bg infra
        # tasks (outbox, drain, preview server, box-health, poller)
        # live in the outer `tasks` list and are gathered below.
        health_tasks: dict[str, asyncio.Task] = {}
        # Audio-reader health tasks (upsell.health liveness logs), keyed like
        # `audio_readers` so hot-swap cancels them individually.
        audio_health_tasks: dict[str, asyncio.Task] = {}

        def _start_reader(cam: CameraConfig) -> bool:
            """Spawn a CameraReader + health task for `cam`.

            Returns False (and logs) if the camera has no `rtsp_url`
            yet — the operator hasn't completed UniFi onboarding for
            it. The next config poll re-tries; cameras that get
            their URL filled in show up as `added_ids` against the
            current_cfg snapshot we hold.
            """
            cid = str(cam.id)
            if not cam.rtsp_url:
                log("info", "edge.reader.skip_no_rtsp", camera_id=cid, name=cam.name)
                return False
            if run_video:
                r = CameraReader(
                    cam,
                    loop,
                    queue,
                    vlm_client,
                    oxy_client=client,
                )
                r.start()
                readers[cid] = r
                health_tasks[cid] = asyncio.create_task(
                    camera_health_loop(client, cam.id, health_interval, r.stats),
                    name=f"camera-health-{cam.name}",
                )
            # Audio upsell reader for allowlisted register cameras. Pulls
            # audio directly from the camera RTSP (MTX strips it), so it's a
            # second, audio-only session to the camera.
            if upsell_stt is not None and upsell.camera_enabled(cam.name, upsell_allow):
                ar = upsell.AudioReader(
                    cam, loop, queue, upsell_stt, upsell_intent_client,
                )
                ar.start()
                audio_readers[cid] = ar
                audio_health_tasks[cid] = asyncio.create_task(
                    audio_health_loop(cam.id, cam.name, health_interval, ar.stats),
                    name=f"audio-health-{cam.name}",
                )
                log("info", "edge.audio_reader.start", camera_id=cid, name=cam.name)
            return True

        def _stop_reader(cid: str) -> None:
            r = readers.pop(cid, None)
            if r is not None:
                r.stop()
            ar = audio_readers.pop(cid, None)
            if ar is not None:
                ar.stop()
            aht = audio_health_tasks.pop(cid, None)
            if aht is not None:
                aht.cancel()
            t = health_tasks.pop(cid, None)
            if t is not None:
                t.cancel()

        for cam in cams:
            _start_reader(cam)

        preview_port = int(os.environ.get("PREVIEW_PORT", "8090"))
        preview_host = os.environ.get("PREVIEW_HOST", "0.0.0.0")
        tasks: list[asyncio.Task] = [
            asyncio.create_task(_outbox_producer(queue, outbox), name="outbox-producer"),
            asyncio.create_task(
                outbox.drain_loop(oxy_url, control_auth),
                name="outbox-drain",
            ),
            asyncio.create_task(
                box_health_loop(client, box_id, health_interval, image_tag),
                name="box-health",
            ),
        ]
        # Preview HTTP server — Oxy backend proxies JPEG snapshots from here for
        # the UI's per-camera thumbnail. Video roles only (audio boxes have no
        # readers to preview); shares the `readers` map so hot-swapped cameras
        # show up / drop out automatically.
        if run_video:
            tasks.append(asyncio.create_task(
                run_preview_server(readers, host=preview_host, port=preview_port),
                name="preview-server",
            ))

        # Log shipper drains the in-memory buffer to Oxy every
        # 30s (or 50 lines, whichever fires first). Reuses the
        # main control-plane `client` so it inherits the bearer
        # + Tailscale headers without duplicating auth setup.
        if log_shipper is not None:
            tasks.append(
                asyncio.create_task(
                    log_shipper.run(client, str(box_id)),
                    name="log-shipper",
                )
            )

        # Bandwidth visibility — only meaningful when MTX is wired
        # up and Funnel exposes it publicly. The reporter logs are
        # cheap to ship to Airhouse later for fleet aggregation;
        # for Phase 1 they sit in the box's log stream.
        if mtx is not None and edge_funnel_hostname:
            tasks.append(
                asyncio.create_task(
                    _bandwidth_reporter(mtx, client, box_id, edge_funnel_hostname),
                    name="bandwidth-reporter",
                )
            )

        current_cfg = {str(c.id): c for c in cams}

        def _on_change(new: list[CameraConfig]) -> None:
            """Reconcile the running READER set with `new` — only
            invoked when the config diff is non-empty. MTX path
            reconcile lives in `_on_every_poll` below because MTX
            is stateful and may have been restarted out from under
            us, so a "no diff in Oxy" poll cycle is exactly when
            we'd otherwise miss re-pushing paths.

            Three classes of reader changes:
              - **removed**: camera no longer in Oxy's config →
                stop the reader thread, cancel the health task.
              - **added**: new camera id → spawn a fresh reader +
                health task.
              - **changed** (same id, different signature →
                see `_cam_signature`): stop the old reader and
                start a new one with the updated config. We don't
                try to mutate the reader in-place — restarts are
                cheap and avoid a whole class of race conditions
                in OpenCV + the tracker.
            """
            new_map = {str(c.id): c for c in new}
            removed_ids = set(current_cfg.keys()) - set(new_map.keys())
            added_ids = set(new_map.keys()) - set(current_cfg.keys())
            changed_ids = {
                cid for cid in new_map.keys() & current_cfg.keys()
                if _cam_signature(new_map[cid]) != _cam_signature(current_cfg[cid])
            }

            for cid in removed_ids:
                log("info", "edge.reader.stop", camera_id=cid, reason="removed")
                _stop_reader(cid)
            for cid in changed_ids:
                log("info", "edge.reader.restart", camera_id=cid, reason="config-changed")
                _stop_reader(cid)
                _start_reader(new_map[cid])
            for cid in added_ids:
                cam = new_map[cid]
                log("info", "edge.reader.start", camera_id=cid, name=cam.name)
                _start_reader(cam)

        def _on_every_poll(new: list[CameraConfig]) -> None:
            """Fires on every successful config poll, regardless of
            whether anything changed. Owns the MTX path reconcile,
            which has to run unconditionally so an MTX restart
            (config change, image bump, crash + auto-restart) gets
            the worker's paths re-pushed within one poll cycle —
            otherwise live preview silently breaks until someone
            notices and bounces the worker.

            mtx_sync.reconcile is idempotent: existing-and-correct
            paths log as `kept`, new ones as `added`, stale ones as
            `removed`. Steady state is one cheap GET against MTX's
            v3 API.
            """
            if mtx is None:
                return
            with_rtsp = [c for c in new if c.rtsp_url]
            asyncio.create_task(
                mtx_sync.reconcile(mtx, with_rtsp),
                name="mtx-resync-every-poll",
            )

        tasks.append(
            asyncio.create_task(
                _config_poller(
                    client,
                    box_id,
                    config_interval,
                    current_cfg,
                    _on_change,
                    _on_every_poll,
                ),
                name="config-poller",
            )
        )

        try:
            await asyncio.gather(*tasks)
        finally:
            for r in readers.values():
                r.stop()
            for ar in audio_readers.values():
                ar.stop()


def main() -> None:
    try:
        asyncio.run(run())
    except StartupError as e:
        log("error", "edge.startup_failed", reason=str(e))
        raise SystemExit(2)
    except KeyboardInterrupt:
        log("info", "edge.shutdown", reason="sigint")


if __name__ == "__main__":
    main()
