#!/usr/bin/env bash
#
# Pull training frames from the video-poc edge stack for Roboflow upload.
#
# Three sources, in priority order:
#   1. A `<source>` arg (RTSP / HTTP URL / local file path) — use as-is.
#   2. MediaMTX paths discovered via the v3 API on localhost:9997. The
#      script lists what's active and pulls live RTSP from each path.
#   3. Sample MP4s under `central/mediamtx/samples/` — fallback when
#      MTX isn't running and no source is given.
#
# Output: ./training-frames/<source-name>/frame_<index>.jpg
#
# Defaults are tuned for a v0 PPE dataset (~150-300 frames per camera,
# spaced every 10s over 30min):
#   * Diverse enough to capture lighting / staff-rotation variation.
#   * Small enough to upload to Roboflow in one go (a couple hundred
#     MB of JPEGs).
#
# Tune via env:
#   INTERVAL_S       seconds between frames (default 10)
#   DURATION_S       total capture duration per source (default 1800 = 30min)
#   MAX_FRAMES       hard cap per source (default 300)
#   JPEG_QUALITY     ffmpeg -q:v (1=best, 31=worst, default 3)
#   OUT_DIR          output root (default ./training-frames)
#   MTX_API          MediaMTX API base (default http://localhost:9997)
#   MTX_RTSP_BASE    RTSP base for live pull (default rtsp://localhost:8554)
#
# Usage:
#   scripts/extract-training-frames.sh                       # auto-discover from MTX
#   scripts/extract-training-frames.sh cam-protect-01        # one MTX path
#   scripts/extract-training-frames.sh rtsp://10.0.0.5/live  # direct RTSP
#   scripts/extract-training-frames.sh central/mediamtx/samples/foo.mp4
#   DURATION_S=600 INTERVAL_S=5 scripts/extract-training-frames.sh
#
# After running, zip the output and upload to Roboflow:
#   cd training-frames && zip -r ../training-frames.zip . && cd ..
# then in your Roboflow project: Upload → drag the zip → annotate.

set -euo pipefail

INTERVAL_S="${INTERVAL_S:-10}"
DURATION_S="${DURATION_S:-1800}"
MAX_FRAMES="${MAX_FRAMES:-300}"
JPEG_QUALITY="${JPEG_QUALITY:-3}"
OUT_DIR="${OUT_DIR:-./training-frames}"
MTX_API="${MTX_API:-http://localhost:9997}"
MTX_RTSP_BASE="${MTX_RTSP_BASE:-rtsp://localhost:8554}"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"

# ─────────────────────────────────────────────────────────────────────
# Preflight
# ─────────────────────────────────────────────────────────────────────

if ! command -v ffmpeg >/dev/null 2>&1; then
  cat <<NO_FFMPEG >&2
error: ffmpeg not on PATH.

Install:
  macOS:  brew install ffmpeg
  Linux:  apt install ffmpeg     # or your distro's equivalent

Or run inside the MediaMTX container (already ships ffmpeg):
  docker exec video-poc-mediamtx ffmpeg ...
(the script doesn't do this automatically because writing the output
 back to the host needs a bind mount — easier to install ffmpeg).
NO_FFMPEG
  exit 1
fi

mkdir -p "$OUT_DIR"

# ─────────────────────────────────────────────────────────────────────
# Source discovery
# ─────────────────────────────────────────────────────────────────────
#
# Resolves the arg into a list of `name|input` lines on stdout.
# `name` becomes the output subdir; `input` is whatever ffmpeg can read.

discover_sources() {
  local arg="${1:-}"

  if [[ -n "$arg" ]]; then
    case "$arg" in
      rtsp://*|rtsps://*|rtmp://*|http://*|https://*)
        # Direct URL — name = host segment, sanitized.
        local name
        name="$(printf '%s' "$arg" \
          | sed -e 's|^[a-z]*://||' -e 's|[^A-Za-z0-9._-]|_|g' \
          | cut -c1-64)"
        printf '%s|%s\n' "$name" "$arg"
        return 0
        ;;
      /*|./*|../*)
        # Local file path.
        [[ -f "$arg" ]] || { echo "error: file not found: $arg" >&2; return 1; }
        local name
        name="$(basename "${arg%.*}")"
        printf '%s|%s\n' "$name" "$arg"
        return 0
        ;;
      *)
        # Assume MTX path name (e.g. cam-protect-01, cam-<uuid>).
        printf '%s|%s/%s\n' "$arg" "$MTX_RTSP_BASE" "$arg"
        return 0
        ;;
    esac
  fi

  # No arg — try MTX API, then samples.
  if command -v curl >/dev/null 2>&1 \
       && PATHS_JSON="$(curl -fsS "${MTX_API}/v3/paths/list" 2>/dev/null)"; then
    # Pull every active path name. MTX v3 returns:
    #   {"itemCount":N,"pageCount":1,"items":[{"name":"cam-…","ready":true,…}]}
    local names
    names="$(printf '%s' "$PATHS_JSON" | python3 -c '
import json, sys
data = json.load(sys.stdin)
for item in data.get("items", []):
    if item.get("ready"):
        print(item["name"])
')"
    if [[ -n "$names" ]]; then
      while IFS= read -r n; do
        printf '%s|%s/%s\n' "$n" "$MTX_RTSP_BASE" "$n"
      done <<<"$names"
      return 0
    fi
    echo "warn: MTX is reachable but has no ready paths — falling back to samples" >&2
  fi

  # Last resort: sample MP4s. Operator-supplied, so we just glob.
  local samples_dir="${PROJECT_DIR}/central/mediamtx/samples"
  local any=0
  shopt -s nullglob
  for f in "$samples_dir"/*.mp4 "$samples_dir"/*.mov "$samples_dir"/*.mkv; do
    any=1
    local name
    name="$(basename "${f%.*}")"
    printf '%s|%s\n' "$name" "$f"
  done
  shopt -u nullglob
  if [[ $any -eq 0 ]]; then
    cat <<NO_SOURCES >&2
error: no sources discovered.

Options:
  • Start the edge stack so MTX has paths:
      make edge
  • Pass a stream URL or MTX path explicitly:
      scripts/extract-training-frames.sh rtsp://10.0.0.5/live
  • Drop sample MP4s into central/mediamtx/samples/
NO_SOURCES
    return 1
  fi
}

# ─────────────────────────────────────────────────────────────────────
# Extract
# ─────────────────────────────────────────────────────────────────────

extract_one() {
  local name="$1"
  local input="$2"
  local out_subdir="${OUT_DIR}/${name}"
  mkdir -p "$out_subdir"

  local existing
  existing="$(find "$out_subdir" -maxdepth 1 -name 'frame_*.jpg' 2>/dev/null | wc -l | tr -d ' ')"
  if [[ "$existing" -ge "$MAX_FRAMES" ]]; then
    echo "  ✓ ${name}: already have ${existing} frames in ${out_subdir} (cap=${MAX_FRAMES})"
    return 0
  fi

  echo "→ ${name}"
  echo "  source: ${input}"
  echo "  → ${out_subdir}/frame_NNNN.jpg"

  # ffmpeg args:
  #   -t DURATION_S            stop after N seconds of input
  #   -vf fps=1/INTERVAL_S     pick one frame every N input seconds
  #   -frames:v MAX_FRAMES     hard cap on output count
  #   -q:v JPEG_QUALITY        JPEG quality (1=best, 31=worst)
  #   -hide_banner -loglevel warning   keep stdout clean for the loop
  #
  # `-rtsp_transport tcp` is required for most production cameras —
  # UDP RTSP through Docker NAT loses packets badly. Harmless for
  # file inputs (ignored by the demuxer).
  ffmpeg \
    -hide_banner -loglevel warning \
    -rtsp_transport tcp \
    -t "$DURATION_S" \
    -i "$input" \
    -vf "fps=1/${INTERVAL_S}" \
    -frames:v "$MAX_FRAMES" \
    -q:v "$JPEG_QUALITY" \
    "${out_subdir}/frame_%04d.jpg" \
    < /dev/null  # silence stdin so ffmpeg doesn't block on tty
  local rc=$?
  if [[ $rc -ne 0 ]]; then
    echo "  ! ffmpeg exited ${rc} on ${name} — partial frames may remain in ${out_subdir}" >&2
    return $rc
  fi
  local count
  count="$(find "$out_subdir" -maxdepth 1 -name 'frame_*.jpg' | wc -l | tr -d ' ')"
  echo "  ✓ ${name}: ${count} frames"
}

# ─────────────────────────────────────────────────────────────────────
# Main
# ─────────────────────────────────────────────────────────────────────

echo "video-poc training-frame extractor"
echo "  interval:   ${INTERVAL_S}s between frames"
echo "  duration:   ${DURATION_S}s per source (≈$((DURATION_S/60))min)"
echo "  cap:        ${MAX_FRAMES} frames per source"
echo "  output:     ${OUT_DIR}"
echo

SOURCES="$(discover_sources "${1:-}")"
if [[ -z "$SOURCES" ]]; then
  exit 1
fi

while IFS='|' read -r NAME INPUT; do
  [[ -z "$NAME" ]] && continue
  extract_one "$NAME" "$INPUT" || true
done <<<"$SOURCES"

TOTAL="$(find "$OUT_DIR" -maxdepth 2 -name 'frame_*.jpg' | wc -l | tr -d ' ')"
echo
echo "✓ ${TOTAL} frames written under ${OUT_DIR}/"
echo
echo "Next steps (Roboflow upload):"
echo "  1. (Optional) Filter out blank frames yourself or let Roboflow's"
echo "     Smart Polygon / active-learning surface the useful ones."
echo "  2. Zip and upload:"
echo "       (cd ${OUT_DIR} && zip -qr ../training-frames.zip .)"
echo "     Then drag training-frames.zip into your Roboflow project."
echo "  3. Annotate the PPE classes you want (hat, hairnet, apron,"
echo "     glove, mask). Roboflow auto-pretrain can suggest boxes from"
echo "     COCO-pretrained YOLOv8 to speed the first pass."
echo "  4. Generate a version → Train (the free credits cover v0)."
echo "  5. Rerun scripts/download-ppe-model.sh to pull the trained .pt."
