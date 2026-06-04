#!/usr/bin/env bash
#
# Drop a PPE-YOLO model into ./models/ for the edge worker's Option C
# bbox overlay. Idempotent — re-runs only download when the target
# file is missing.
#
# ─────────────────────────────────────────────────────────────────────
# Default: food-industry-ppe (Roboflow Universe)
# https://universe.roboflow.com/proof-87nsb/food-industry-ppe
# ─────────────────────────────────────────────────────────────────────
#
# Roboflow Universe models require a (free) Roboflow account + API key
# because the trained weights are served from a per-export presigned
# S3 URL, not a public static asset. Two paths:
#
#   (a) Automated — set ROBOFLOW_API_KEY in your env / .env:
#         export ROBOFLOW_API_KEY=…
#         scripts/download-ppe-model.sh
#       (get the key at https://app.roboflow.com → Settings → Workspace
#        API Key. The free plan covers private/personal use.)
#
#   (b) Manual — open the model in a browser:
#         https://universe.roboflow.com/proof-87nsb/food-industry-ppe
#       Click "Export Dataset" → "Get Snapshot" → "Download YOLOv8 model
#       weights." Save the `.pt` into `video-poc/models/` and add to .env:
#         PPE_YOLO_MODEL=/app/models/<filename>.pt
#
# Override the source by passing an HTTPS URL as the first argument
# (must resolve to an Ultralytics-compatible .pt). Falls back to a
# HuggingFace construction-PPE model for users who just want a
# zero-config plumbing test (`URL=construction`).
#
# Usage:
#   scripts/download-ppe-model.sh                       # food-industry-ppe via Roboflow (needs ROBOFLOW_API_KEY)
#   ROBOFLOW_API_KEY=… scripts/download-ppe-model.sh
#   scripts/download-ppe-model.sh construction          # HuggingFace yolov8n-construction-safety
#   scripts/download-ppe-model.sh https://…/my.pt       # arbitrary URL
#
# After running, set in .env:
#   PPE_YOLO_MODEL=/app/models/<filename>.pt
# and `docker compose --profile edge up -d edge` to pick it up.

set -euo pipefail

# Roboflow Universe coordinates for the default food-industry PPE model.
# Override via env if pointing at a different project (e.g. your own
# finetuned version).
RF_WORKSPACE="${RF_WORKSPACE:-tays-workspace-crpbz}"
RF_PROJECT="${RF_PROJECT:-food_ppe_poc}"
RF_VERSION="${RF_VERSION:-1}"

# Construction-PPE fallback — HuggingFace, public, no auth.
FALLBACK_URL="https://huggingface.co/keremberke/yolov8n-construction-safety/resolve/main/best.pt"
FALLBACK_FILENAME="yolov8n-construction-safety.pt"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TARGET_DIR="${SCRIPT_DIR}/../models"
mkdir -p "$TARGET_DIR"

# ─────────────────────────────────────────────────────────────────────
# Dispatch on argument
# ─────────────────────────────────────────────────────────────────────

ARG="${1:-}"

if [[ "$ARG" == "construction" ]]; then
  MODE="fallback"
elif [[ "$ARG" == http* ]]; then
  MODE="direct"
  DIRECT_URL="$ARG"
elif [[ -z "$ARG" ]]; then
  MODE="roboflow"
else
  echo "error: unrecognized arg: $ARG" >&2
  echo "  use no arg (Roboflow default), 'construction' (HF fallback), or an https URL." >&2
  exit 1
fi

# ─────────────────────────────────────────────────────────────────────
# Download helpers
# ─────────────────────────────────────────────────────────────────────

http_get() {
  # Streams to stdout. Used for both the model file and the small
  # Roboflow export-metadata JSON. Prefers curl, falls back to wget.
  local url="$1"
  if command -v curl >/dev/null 2>&1; then
    curl --fail --silent --location "$url"
  elif command -v wget >/dev/null 2>&1; then
    wget --quiet --output-document=- "$url"
  else
    echo "error: need curl or wget on PATH" >&2
    return 1
  fi
}

http_get_to_file() {
  local url="$1"
  local out="$2"
  if command -v curl >/dev/null 2>&1; then
    curl --fail --location --progress-bar --output "$out" "$url"
  else
    wget --show-progress --output-document="$out" "$url"
  fi
}

sanity_check_size() {
  # HuggingFace + Roboflow occasionally serve a JSON error envelope or
  # an HTML page when something's wrong. yolov8n weights are ~6MB;
  # <100KB is almost certainly not weights. Preserve the body as
  # `<path>.unexpected` and dump the head of it so the caller can see
  # the real diagnostic (Roboflow returns very useful JSON errors).
  local path="$1"
  local size
  size="$(wc -c <"$path" | tr -d ' ')"
  if [[ "$size" -lt 102400 ]]; then
    local saved="${path}.unexpected"
    mv "$path" "$saved"
    echo "error: download returned ${size}B — not weights." >&2
    echo "  saved body to: ${saved}" >&2
    echo "  --- first 1500 bytes ---" >&2
    head -c 1500 "$saved" >&2
    echo >&2
    echo "  --- end body ---" >&2
    echo >&2
    echo "Common causes (in order of likelihood):" >&2
    echo "  • This version has no trained weights — Roboflow's /yolov8" >&2
    echo "    endpoint gives the DATASET (+ weights if trained on" >&2
    echo "    Roboflow Train). Check https://app.roboflow.com →" >&2
    echo "    your project → 'Models' tab. If empty, train one (the" >&2
    echo "    free Train credits work fine for a v0)." >&2
    echo "  • Wrong format — try yolov11/yolov5 by editing the URL" >&2
    echo "    in this script (the format suffix after /version/)." >&2
    echo "  • API key invalid or expired." >&2
    exit 1
  fi
}

emit_next_steps() {
  local filename="$1"
  echo
  echo "Next steps:"
  echo "  1. Add to .env:"
  echo "       PPE_YOLO_MODEL=/app/models/${filename}"
  echo "  2. Restart the edge service:"
  echo "       docker compose --profile edge up -d edge"
  echo "  3. Trigger a compliance event and open Playback with ?debug=1"
  echo "     to see the bbox overlay."
}

# ─────────────────────────────────────────────────────────────────────
# Direct URL mode
# ─────────────────────────────────────────────────────────────────────

if [[ "$MODE" == "direct" ]]; then
  FILENAME="$(basename "${DIRECT_URL%%\?*}")"
  if [[ -z "$FILENAME" || "$FILENAME" != *.pt ]]; then
    echo "error: could not derive a .pt filename from URL: $DIRECT_URL" >&2
    exit 1
  fi
  TARGET="${TARGET_DIR}/${FILENAME}"
  if [[ -f "$TARGET" ]]; then
    echo "✓ already present: ${TARGET} ($(du -h "$TARGET" | cut -f1))"
    exit 0
  fi
  echo "→ fetching ${DIRECT_URL}"
  http_get_to_file "$DIRECT_URL" "$TARGET"
  sanity_check_size "$TARGET"
  echo "✓ downloaded ${FILENAME} ($(du -h "$TARGET" | cut -f1))"
  emit_next_steps "$FILENAME"
  exit 0
fi

# ─────────────────────────────────────────────────────────────────────
# Construction fallback (HuggingFace)
# ─────────────────────────────────────────────────────────────────────

if [[ "$MODE" == "fallback" ]]; then
  TARGET="${TARGET_DIR}/${FALLBACK_FILENAME}"
  if [[ -f "$TARGET" ]]; then
    echo "✓ already present: ${TARGET} ($(du -h "$TARGET" | cut -f1))"
    exit 0
  fi
  echo "→ fetching construction-PPE fallback from HuggingFace"
  echo "  (this is a plumbing-test default — restaurant ontology lives"
  echo "   in the Roboflow food-industry-ppe model; rerun without args"
  echo "   once you have ROBOFLOW_API_KEY set)"
  http_get_to_file "$FALLBACK_URL" "$TARGET"
  sanity_check_size "$TARGET"
  echo "✓ downloaded ${FALLBACK_FILENAME} ($(du -h "$TARGET" | cut -f1))"
  emit_next_steps "$FALLBACK_FILENAME"
  exit 0
fi

# ─────────────────────────────────────────────────────────────────────
# Roboflow Universe (default — food-industry-ppe)
# ─────────────────────────────────────────────────────────────────────

cat <<HEADER
→ Roboflow Universe: ${RF_WORKSPACE}/${RF_PROJECT} v${RF_VERSION}
  (food-industry-ppe — hat, hairnet, apron, glove, mask)

HEADER

if [[ -z "${ROBOFLOW_API_KEY:-}" ]]; then
  cat <<NO_KEY >&2
ROBOFLOW_API_KEY is not set — Roboflow Universe requires a free API key.

Two options:

  (a) Automated. Get the key:
        https://app.roboflow.com → Settings → Workspace → API Key
      then re-run:
        ROBOFLOW_API_KEY=… scripts/download-ppe-model.sh

  (b) Manual. Open the model in a browser:
        https://universe.roboflow.com/${RF_WORKSPACE}/${RF_PROJECT}
      Click "Export Dataset" → choose YOLOv8 → "Get Weights",
      save the .pt into video-poc/models/, then:
        PPE_YOLO_MODEL=/app/models/<filename>.pt   # in .env

  (c) Plumbing test only. Use the construction-PPE fallback:
        scripts/download-ppe-model.sh construction

NO_KEY
  exit 2
fi

# Roboflow exports are asynchronous: the first call to the YOLOv8
# format endpoint queues a server-side conversion and returns
# `{"progress": 0}`. Subsequent calls report progress until the
# weights are ready, at which point the response shape becomes:
#   { "export": { "link": "https://.../weights.pt?…" }, … }
# Wait up to RF_EXPORT_TIMEOUT_S (default 5min) with a 10s poll.
# Once the link arrives it's usable for ~5 minutes — fetch + use
# immediately.
META_URL="https://api.roboflow.com/${RF_WORKSPACE}/${RF_PROJECT}/${RF_VERSION}/yolov8?api_key=${ROBOFLOW_API_KEY}"
RF_EXPORT_TIMEOUT_S="${RF_EXPORT_TIMEOUT_S:-300}"
RF_POLL_INTERVAL_S="${RF_POLL_INTERVAL_S:-10}"

FILENAME="${RF_PROJECT}-v${RF_VERSION}.pt"
TARGET="${TARGET_DIR}/${FILENAME}"
if [[ -f "$TARGET" ]]; then
  echo "✓ already present: ${TARGET} ($(du -h "$TARGET" | cut -f1))"
  echo "  (delete the file to re-download)"
  exit 0
fi

# Tiny extractor — pulls export.link OR a `progress` field, with a
# distinct exit code per shape so the caller knows whether to keep
# polling or give up. Avoids a jq dep.
parse_roboflow_response() {
  python3 - "$1" <<'PY'
import json, sys
raw = sys.argv[1]
try:
    data = json.loads(raw)
except json.JSONDecodeError as e:
    sys.stderr.write(f"Roboflow returned non-JSON: {e}\n{raw[:512]}\n")
    sys.exit(4)
link = (data.get("export") or {}).get("link")
if link:
    print(link)
    sys.exit(0)
if "progress" in data:
    prog = data["progress"]
    sys.stderr.write(f"export queued, progress={prog}\n")
    sys.exit(2)  # poll again
# Any other shape — surface what came back so the operator can
# triage (most common: API key invalid, version doesn't exist).
sys.stderr.write("Roboflow response missing export.link AND progress:\n")
sys.stderr.write(json.dumps(data, indent=2)[:512] + "\n")
sys.exit(3)
PY
}

DEADLINE=$(( $(date +%s) + RF_EXPORT_TIMEOUT_S ))
ATTEMPT=0
EXPORT_URL=""
while :; do
  ATTEMPT=$(( ATTEMPT + 1 ))
  if [[ $ATTEMPT -eq 1 ]]; then
    echo "  triggering export…"
  else
    NOW_LEFT=$(( DEADLINE - $(date +%s) ))
    echo "  poll #${ATTEMPT} (timeout in ${NOW_LEFT}s)…"
  fi
  META_JSON="$(http_get "$META_URL")"
  set +e
  EXPORT_URL="$(parse_roboflow_response "$META_JSON" 2>/tmp/rf_msg.$$)"
  STATUS=$?
  set -e
  case "$STATUS" in
    0)
      rm -f /tmp/rf_msg.$$
      break
      ;;
    2)
      # Still queued — wait + retry until the deadline.
      cat /tmp/rf_msg.$$ >&2 || true
      rm -f /tmp/rf_msg.$$
      if [[ $(date +%s) -ge $DEADLINE ]]; then
        echo "error: export still not ready after ${RF_EXPORT_TIMEOUT_S}s" >&2
        echo "  retry later, or bump RF_EXPORT_TIMEOUT_S=600 scripts/download-ppe-model.sh" >&2
        exit 1
      fi
      sleep "$RF_POLL_INTERVAL_S"
      ;;
    *)
      # Shape we don't recognize — already logged the response body.
      cat /tmp/rf_msg.$$ >&2 || true
      rm -f /tmp/rf_msg.$$
      exit "$STATUS"
      ;;
  esac
done

echo "  → ${TARGET}"
http_get_to_file "$EXPORT_URL" "$TARGET"
sanity_check_size "$TARGET"
echo "✓ downloaded ${FILENAME} ($(du -h "$TARGET" | cut -f1))"
emit_next_steps "$FILENAME"
