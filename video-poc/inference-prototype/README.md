# Inference prototype

The two-stage YOLO + VLM pipeline that will become the edge worker's inference
loop. Lives here as a single runnable script so we can validate behavior end-to-end
against pre-recorded footage before grafting it into `video-poc/edge/worker/`.

## What's in here

- **`protocol_compliance_oxy.py`** — the full pipeline:
  - YOLO11 nano + ByteTrack + supervision (PolygonZone, LineZone) per frame
  - Emits `enter` / `exit` / `dwell` / `line_cross` rows to `camera_events`
  - **On-trigger** Claude Haiku compliance check when a track sustains the PPE
    zone for `TRIGGER_DWELL_SEC` (default 10s), gated by per-track and global
    cooldowns. Result lands in `camera_compliance_reports`.
- **`oxy_emit.py`** — thin httpx client that talks to the central control-API
  (`/control/register`, `/control/events`, `/control/compliance-reports`,
  `/control/cameras/{id}/zones`). The same emit pattern will be inlined into the
  edge worker's outbox-and-drain loop once we lift the inference logic in.
- **`requirements.txt`** — minimal deps to run the prototype.
- **`.env.example`** — copy to `.env`, set `ANTHROPIC_API_KEY` + `VIDEO_PATH`.

## Run

```bash
# 1. Central stack (separate shell)
cd ../  # i.e. video-poc/
make up

# 2. Prototype env
cd inference-prototype
python -m venv .venv
source .venv/bin/activate
pip install -r requirements.txt
cp .env.example .env       # then edit: ANTHROPIC_API_KEY + VIDEO_PATH

# 3. Run against a video
python protocol_compliance_oxy.py --video /path/to/footage.mp4
# OR set VIDEO_PATH in .env and just:
python protocol_compliance_oxy.py
```

## Verify

In `psql` (from the host: `cd .. && make psql`):

```sql
-- Events landed?
SELECT event_type, COUNT(*) FROM camera_events
WHERE camera_id = '55555555-5555-5555-5555-555555555555'
GROUP BY event_type;

-- Compliance reports + parsed verdicts + cost
SELECT segment_start, trigger_track_id, vlm_model, tokens_used,
       structured_json->>'attire_compliant'  AS attire_ok,
       structured_json->>'hygiene_compliant' AS hygiene_ok,
       structured_json->>'missing_items'     AS missing
FROM camera_compliance_reports
ORDER BY received_at DESC LIMIT 10;
```

## Path to production

This script is intentionally a single file with a synchronous frame loop. The
production edge worker (`video-poc/edge/worker/`) is async-first with a durable
SQLite outbox and per-camera threads. Migration plan:

1. Extract the per-frame inference into a function the edge worker's
   `CameraReader` can call instead of its current synthetic-event stub.
2. Run the VLM call via `asyncio.create_task` so the frame loop doesn't block
   on the 2–5s Haiku roundtrip.
3. Route `emit_event` / `emit_compliance_report` through the existing outbox
   (in `edge/worker/outbox.py`) instead of the direct httpx client here.
4. Swap `sv.get_video_frames_generator(file)` for an `cv2.VideoCapture` read
   loop against `rtsp://mediamtx:8554/<path>`.

Once the edge worker has feature parity, this directory can be deleted.
