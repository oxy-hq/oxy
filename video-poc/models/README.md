# PPE-YOLO models

This directory is mounted into the edge worker at `/app/models/` (read-only).
Drop `.pt` files here and point `PPE_YOLO_MODEL` at one in `.env` to enable
the Option C bbox overlay.

## Recommended: train your own (Roboflow free plan)

The Roboflow free plan supports dataset export but **not weight download**.
Train locally instead — it's ~10 min on an M-series Mac via MPS — and you
get a model you fully own. End-to-end:

```bash
# 1. Capture training data from your live cameras (~30 min wall-clock).
make training-frames SOURCE=cam-<id>           # auto-discovers MTX paths

# 2. Zip + upload + annotate in Roboflow (browser).
(cd training-frames && zip -qr ../training-frames.zip .)
# → upload training-frames.zip to your Roboflow project, label PPE
#   classes (hat, hairnet, apron, glove, mask), Generate version.

# 3. Train locally from the annotated dataset.
ROBOFLOW_API_KEY=… make train-ppe              # ~5–15 min on MPS
# Drops <project>-trained.pt into ./models/

# 4. Enable on the edge.
# .env:
#   PPE_YOLO_MODEL=/app/models/food_ppe-trained.pt
docker compose --profile edge up -d edge
```

`make train-ppe` creates a one-time Python venv with ultralytics + the
Roboflow SDK, pulls the annotated dataset, fine-tunes COCO-pretrained
YOLOv8n, and copies `best.pt` into `./models/`. Tunables: `EPOCHS`,
`BATCH`, `IMG_SIZE`, `DEVICE` — defaults are calibrated for 100–500
frames on Apple Silicon.

## Public pretrained models (no training required)

If you already have a public PPE model whose ontology fits, the
download script bypasses training entirely. The catch: most public
Roboflow Universe models are dataset-only on the free plan — you'd
hit the same weight-download wall. The reliable public source is
HuggingFace.

```bash
# Construction-PPE (hardhat / vest / mask) — fine for plumbing-test,
# wrong ontology for food service.
scripts/download-ppe-model.sh construction
```

## Plumbing test: construction-PPE (HuggingFace)

No Roboflow account required — useful when you just want to verify the
overlay path end-to-end before doing the food-industry setup.

```bash
scripts/download-ppe-model.sh construction
# PPE_YOLO_MODEL=/app/models/yolov8n-construction-safety.pt   # in .env
```

## Custom / finetuned model

Any Ultralytics-compatible `.pt` (YOLOv5 / v8 / v11) works. Drop the file in
and point `PPE_YOLO_MODEL` at its in-container path.

```bash
cp ~/Downloads/my-restaurant-ppe.pt models/
# PPE_YOLO_MODEL=/app/models/my-restaurant-ppe.pt   # in .env
```

`scripts/download-ppe-model.sh https://…/file.pt` also takes any HTTPS URL.

## Why this dir is gitignored

Model weights are 6–200MB and update on their own cadence. Track *what model
is in use* via the envelope's `model` field on each compliance report (the
Playback page surfaces it in the YOLO debug badge); the weights themselves
live only on the host.

## Per-domain models (roadmap)

Today the model is a single global env var on each worker. The natural next
step is to push the model reference into the **domain pack** that already
drives VLM prompts, so switching a workspace from "restaurant" to
"construction" auto-updates YOLO weights on every box.
