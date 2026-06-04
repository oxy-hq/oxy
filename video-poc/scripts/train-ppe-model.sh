#!/usr/bin/env bash
#
# Train a PPE-YOLO model locally from an annotated Roboflow dataset.
#
# Roboflow's free plan blocks trained-weight download but allows
# dataset export. So we:
#   1. Pull the YOLOv8-format dataset (images + labels.txt + data.yaml)
#      from Roboflow into ./training-data/.
#   2. Fine-tune a COCO-pretrained YOLOv8n with Ultralytics — the same
#      ultralytics package the edge container already runs for person
#      tracking, no new dependencies in the worker.
#   3. Copy `runs/detect/train/weights/best.pt` into `./models/` so
#      the existing `PPE_YOLO_MODEL=/app/models/…` env var picks it up.
#
# Pre-requisites:
#   • You've annotated frames in your Roboflow project and generated
#     a version. The "Versions" tab in the Roboflow UI is what you're
#     looking for; the version number drives the dataset URL.
#   • ROBOFLOW_API_KEY in env (the same one `download-ppe-model.sh`
#     uses; free plan covers this).
#   • Python 3.9+ on PATH.
#
# Defaults:
#   EPOCHS=50      good for 100–500 frames; bump to 100 for >500
#   IMG_SIZE=640   YOLOv8 native; smaller boxes lose recall
#   BATCH=16       fits in 8GB unified memory; lower to 8 if it OOMs
#   DEVICE=auto    auto-pick — MPS on Apple Silicon, CUDA if present,
#                  CPU otherwise. Force with DEVICE=cpu / mps / 0.
#
# Override the Roboflow coordinates if you renamed the project or
# bumped the version:
#   RF_WORKSPACE=…  default: tays-workspace-crpbz
#   RF_PROJECT=…    default: food_ppe
#   RF_VERSION=…    default: 1
#
# Usage:
#   ROBOFLOW_API_KEY=… scripts/train-ppe-model.sh
#   ROBOFLOW_API_KEY=… EPOCHS=100 scripts/train-ppe-model.sh
#
# After training, the script writes `./models/<project>-trained.pt`.
# Drop the path into .env as PPE_YOLO_MODEL and restart the edge
# service to pick up the new model.

set -euo pipefail

RF_WORKSPACE="${RF_WORKSPACE:-tays-workspace-crpbz}"
RF_PROJECT="${RF_PROJECT:-food_ppe}"
RF_VERSION="${RF_VERSION:-1}"

EPOCHS="${EPOCHS:-50}"
IMG_SIZE="${IMG_SIZE:-640}"
BATCH="${BATCH:-16}"
DEVICE="${DEVICE:-auto}"
BASE_MODEL="${BASE_MODEL:-yolov8n.pt}"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
VENV_DIR="${PROJECT_DIR}/.training-venv"
DATA_DIR="${PROJECT_DIR}/training-data"
RUNS_DIR="${PROJECT_DIR}/training-runs"
MODELS_DIR="${PROJECT_DIR}/models"
OUT_FILE="${MODELS_DIR}/${RF_PROJECT}-trained.pt"

# ─────────────────────────────────────────────────────────────────────
# Pre-flight
# ─────────────────────────────────────────────────────────────────────

if [[ -z "${ROBOFLOW_API_KEY:-}" ]]; then
  echo "error: ROBOFLOW_API_KEY not set." >&2
  echo "  Get one at https://app.roboflow.com → Settings → Workspace → API Key" >&2
  exit 1
fi

if ! command -v python3 >/dev/null 2>&1; then
  echo "error: python3 not on PATH." >&2
  exit 1
fi

mkdir -p "$DATA_DIR" "$RUNS_DIR" "$MODELS_DIR"

# ─────────────────────────────────────────────────────────────────────
# Auto-detect device — Ultralytics accepts the string, but pre-printing
# what we picked makes log debugging less ambiguous.
# ─────────────────────────────────────────────────────────────────────

resolve_device() {
  if [[ "$DEVICE" != "auto" ]]; then
    echo "$DEVICE"
    return
  fi
  python3 - <<'PY'
import sys
try:
    import torch  # noqa: F401
except Exception:
    print("cpu"); sys.exit(0)
import torch
if torch.cuda.is_available():
    print("0")
elif hasattr(torch.backends, "mps") and torch.backends.mps.is_available():
    print("mps")
else:
    print("cpu")
PY
}

# ─────────────────────────────────────────────────────────────────────
# Venv — keeps the heavy training deps (torch, ultralytics, roboflow)
# out of the system Python. ~1.5GB on first install; reused after.
# ─────────────────────────────────────────────────────────────────────

if [[ ! -d "$VENV_DIR" ]]; then
  echo "→ creating training venv at ${VENV_DIR}"
  python3 -m venv "$VENV_DIR"
fi
# shellcheck source=/dev/null
source "${VENV_DIR}/bin/activate"

# Install only if not already present — re-runs after first setup are
# instant. `roboflow` pulls in opencv/numpy as transitive deps.
if ! python -c "import ultralytics, roboflow" 2>/dev/null; then
  echo "→ installing ultralytics + roboflow (one-time, ~1.5GB of wheels)…"
  pip install --quiet --upgrade pip
  pip install --quiet ultralytics roboflow
fi

DEVICE_RESOLVED="$(resolve_device)"
echo "→ device: ${DEVICE_RESOLVED}"

# ─────────────────────────────────────────────────────────────────────
# Download annotated dataset from Roboflow
# ─────────────────────────────────────────────────────────────────────
#
# The Roboflow Python SDK writes to a `<project>-<version>/` dir
# alongside the cwd by default. We cwd into DATA_DIR so all dataset
# artefacts live under one operator-owned path.

DATASET_DIR="${DATA_DIR}/${RF_PROJECT}-${RF_VERSION}"
if [[ -f "${DATASET_DIR}/data.yaml" ]]; then
  echo "✓ dataset already cached at ${DATASET_DIR}"
  echo "  (delete the dir to re-download)"
else
  echo "→ downloading Roboflow dataset ${RF_WORKSPACE}/${RF_PROJECT} v${RF_VERSION}"
  python - "$RF_WORKSPACE" "$RF_PROJECT" "$RF_VERSION" "$DATA_DIR" <<'PY'
import os, sys
from pathlib import Path
from roboflow import Roboflow

workspace, project, version, target = sys.argv[1:5]
os.chdir(target)
rf = Roboflow(api_key=os.environ["ROBOFLOW_API_KEY"])
proj = rf.workspace(workspace).project(project)
proj.version(int(version)).download("yolov8")
# Ultralytics needs an absolute path to dataset.yaml — the relative
# path that Roboflow's downloader produces resolves against cwd at
# train time, which is fragile when the training script runs from
# a different dir. Patch the data.yaml in place with absolute paths.
ds_dir = Path(target) / f"{project}-{version}"
yaml = ds_dir / "data.yaml"
if yaml.exists():
    text = yaml.read_text()
    out = []
    for line in text.splitlines():
        if line.startswith(("train:", "val:", "test:")):
            key, _, val = line.partition(":")
            val = val.strip()
            if val and not val.startswith("/"):
                val = str((ds_dir / val).resolve())
            out.append(f"{key}: {val}")
        else:
            out.append(line)
    yaml.write_text("\n".join(out) + "\n")
print(f"✓ dataset at {ds_dir}")
PY
fi

DATA_YAML="${DATASET_DIR}/data.yaml"
if [[ ! -f "$DATA_YAML" ]]; then
  echo "error: data.yaml missing at ${DATA_YAML}" >&2
  exit 1
fi

echo "→ class list:"
grep -E "^(names|nc):" "$DATA_YAML" | sed 's/^/    /'

# ─────────────────────────────────────────────────────────────────────
# Train
# ─────────────────────────────────────────────────────────────────────

RUN_NAME="${RF_PROJECT}-v${RF_VERSION}-$(date +%Y%m%d-%H%M%S)"
echo "→ training (epochs=${EPOCHS}, imgsz=${IMG_SIZE}, batch=${BATCH})…"
echo "  output dir: ${RUNS_DIR}/${RUN_NAME}"

yolo train \
  model="$BASE_MODEL" \
  data="$DATA_YAML" \
  epochs="$EPOCHS" \
  imgsz="$IMG_SIZE" \
  batch="$BATCH" \
  device="$DEVICE_RESOLVED" \
  project="$RUNS_DIR" \
  name="$RUN_NAME" \
  exist_ok=False

BEST="${RUNS_DIR}/${RUN_NAME}/weights/best.pt"
if [[ ! -f "$BEST" ]]; then
  echo "error: training finished but ${BEST} missing — check the run log" >&2
  exit 1
fi

cp "$BEST" "$OUT_FILE"
echo
echo "✓ trained model copied to ${OUT_FILE} ($(du -h "$OUT_FILE" | cut -f1))"
echo
echo "Next steps:"
echo "  1. Add to .env:"
echo "       PPE_YOLO_MODEL=/app/models/$(basename "$OUT_FILE")"
echo "  2. Restart the edge service:"
echo "       docker compose --profile edge up -d edge"
echo "  3. Trigger a compliance event and open Playback with ?debug=1"
echo "     to see your trained bboxes overlaid."
echo
echo "Validation metrics for this run live at:"
echo "  ${RUNS_DIR}/${RUN_NAME}/results.png        — loss + mAP curves"
echo "  ${RUNS_DIR}/${RUN_NAME}/confusion_matrix.png"
echo "  ${RUNS_DIR}/${RUN_NAME}/val_batch*_pred.jpg — sanity-check images"
