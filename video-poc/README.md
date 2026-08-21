# video-poc — fleet simulation environment

A one-laptop simulation of the retail-analytics video pipeline. Models a
single edge box (MediaMTX + Python worker) talking to Oxy as the control
plane. The old `central/` stack (FastAPI + Postgres) is gone — Oxy itself
now owns registration, config, events, and compliance reports. The control
plane lives in [`crates/cameras`](../crates/cameras/) in this repo.

Architecture lives at
[`internal-docs/video-processing-fleet-architecture.md`](../internal-docs/video-processing-fleet-architecture.md).

## What's in the box

```
video-poc/
  docker-compose.yml          # profiles: edge | chaos
  Makefile                    # edge / scale / chaos-* / clean
  central/
    mediamtx/mediamtx.yml     # virtual RTSP cameras + recording config
    mediamtx/samples/         # drop sample-01.mp4 + sample-02.mp4 here
  edge/                       # Python worker: read RTSP → outbox → drain
```

## Prereqs

- Docker Desktop (compose v2). macOS Apple Silicon and Linux x86_64 both work.
- Two H.264 MP4 clips at `central/mediamtx/samples/sample-01.mp4` and
  `sample-02.mp4`. See `central/mediamtx/samples/README.md` for sources and the
  one-line ffmpeg transcode if your clips aren't H.264/yuv420p.
- An Oxy backend reachable from the edge container. By default Compose points
  the worker at `http://host.docker.internal:3000/api` — start Oxy locally
  (`cargo run -p oxy-server -- serve`) on port 3000, register an edge box in the
  UI, copy the bearer, and paste it into `.env`.

## Quickstart

```sh
cd video-poc
cp .env.example .env             # then fill in EDGE_BOX_ID + EDGE_BOX_TOKEN

# Start the edge stack (mediamtx + worker). Worker auto-registers
# against Oxy on first boot via its bearer.
make edge

# Watch the worker chew through frames + emit events.
make logs SVC=edge
```

Scale to a five-edge fleet:

```sh
make scale N=5
```

Each replica reads the same bearer from `.env` — for a real multi-box
simulation, register multiple edge boxes in Oxy and run multiple
`docker compose` projects with different `.env` files.

## Chaos workflow

Verify the outbox actually survives network failures:

```sh
# Disconnect edge ↔ Oxy for 60s — events keep producing into SQLite,
# drain backs off, then catches up on reconnect.
make chaos-disconnect

# Inject 30% packet loss for 60s.
make chaos-loss PCT=30

# Add 500ms latency for 60s.
make chaos-latency MS=500 DURATION=60s
```

While chaos is running, tail edge logs (`make logs SVC=edge`) and watch the
`outbox.drain_failed` / `outbox.drained` lines. Then query the
`oxy_cam_*` Airhouse tables from the Oxy UI before, during, and after — the
total should keep climbing once the network restores, with no missing rows.

## What works, what's stubbed

**Works**
- Edge bearer auth → config fetch → per-camera RTSP read loop with real
  YOLO + supervision tracker (Ultralytics).
- SQLite outbox with exactly-once delivery (`ON CONFLICT (event_id) DO NOTHING`).
- Box + camera health heartbeats on a 30s cadence.
- Config polling every 30s, with hot-reload of camera readers when Oxy adds,
  removes, or reconfigures a camera (no worker restart needed).
- MTX recording for clip capture on compliance violations (24h rolling
  fMP4 segments served via the playback endpoint at `:9996`).
- Chaos targets via pumba (`tc` netem under the hood).
- **OTA update agent** (`worker/update_agent.py`, runs as the `update-agent`
  compose service). Polls `/control/boxes/{id}/target` every ~5 min and
  re-pulls + restarts the worker via `docker compose` when the operator
  sets a new `target_image_tag`. Watchdog rollback (#156) reverts when
  the worker doesn't catch up on the new tag; `held_until` (#157) lets
  operators pause updates during "don't touch this box" windows
  (e.g. lunch rush).

**Stubbed / external**
- **TLS.** Edge ↔ Oxy is plain HTTP in dev. Production deploys Tailscale and
  Oxy behind a real cert — see `internal-docs/edge-tls-production.md`.
- **Update-agent release pipeline.** The compose file builds the agent
  from the same Dockerfile as the worker. A future GitHub Actions job
  should publish a tagged `ghcr.io/oxy/edge:vX.Y.Z` image so customers
  pin to a release tag instead of building locally.
