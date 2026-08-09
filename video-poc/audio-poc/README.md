# audio-poc — upsell detection feasibility harness

Detect when a counter **employee makes an upsell offer** ("would you like to add
avocado?") from register-mic audio, so we can attribute it (via Toast) and measure
effectiveness. This package is the **Phase-1 feasibility POC** for the feature spec at
[`internal-docs/2026-08-03-upselling-detection-spec.md`](../../internal-docs/2026-08-03-upselling-detection-spec.md).

```
capture.py    ffmpeg → in-memory PCM (file or register RTSP); nothing written to disk
transcribe.py on-device STT (faster-whisper, CPU int8)
intent.py     Haiku classifier: is this an upsell OFFER? which item?   ← the core novel bit
pipeline.py   orchestrates: source → STT → intent → `upsell_attempt` event
fixtures.py   labeled utterances for scoring the classifier
run.py        CLI
```

## Privacy posture

The register cameras (`PH - Santa Clara - Register`, `PH-ALMADEN-Register`) use a mic
**tuned to the employee side** — we record staff, not customers. On top of that the
pipeline is **transcribe-and-discard**: PCM flows through memory to STT and is dropped;
**no audio or transcript is written to disk or sent off-box**, only the derived
`upsell_attempt` signal. Enable audio on the **register camera only**. (Any move toward
live customer audio would need the §632 consent story in the spec.)

## Run

```bash
pip install -r requirements.txt   # ffmpeg must be on PATH

# 1) score the intent classifier (needs ANTHROPIC_API_KEY; runs anywhere)
python run.py eval-intent

# 2) full pipeline on a CONSENTED recording, then the live mic (runs on the edge box)
python run.py detect --source ./consented_session.wav      --camera-id ph-sc-register
python run.py detect --source rtsp://<register-cam>/<alias> --camera-id ph-sc-register
```

`eval-intent` needs only the Anthropic key. `detect` needs `faster-whisper` (STT) and is
meant to run on the edge box against a **consented session first**, then the register mic.

## Status

- ✅ **Intent layer** validated on the fixture set (see `run.py eval-intent`): Haiku
  cleanly separates offers from order-taking and bare item mentions. This is a sanity
  check on ~26 hand-written lines, **not** a rigorous eval.
- ⏳ **STT-on-real-audio** — the other feasibility unknown; validate on a consented
  register recording (accuracy of `faster-whisper` on noisy counter audio).
- ⏳ **Wire to the pipeline** — Phase 2: replace `pipeline._emit`'s print with a
  `POST /control/events` through the edge outbox (same event shape), then the rollup +
  dashboard per the spec.

## Notes

- Windowing is fixed-length (`--window-sec`, default 12s) with a per-item cooldown to
  collapse an offer that straddles two windows. A VAD-segmented version is a later
  refinement.
- `UPSELL_INTENT_MODEL` / `UPSELL_STT_MODEL` env vars override the models.
