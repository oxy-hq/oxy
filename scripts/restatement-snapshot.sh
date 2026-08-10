#!/usr/bin/env bash
#
# Snapshot every monitored measure's trailing buckets, so restatement reach can
# be *measured* instead of assumed.
#
# The 21d / 28d / 7d restatement values in `.monitor.yml` were sized from a
# single ~4-hour observation window. That bounds restatement from below and says
# nothing about the maximum, so the 2x margin they carry is a guess. The only
# way to do better is a time series of the same bucket observed on different
# days — which is the one thing nothing in the product records, because each
# scan overwrites its own view of history.
#
# This script has exactly one job: make that series exist. No analysis, no
# thresholds, no verdict. Run it on a cron; after a few weeks the diffs between
# consecutive snapshots *are* the reach distribution.
#
# Usage:
#   OXY_BASE_URL=https://app.oxygen-hq.com \
#   OXY_WORKSPACE_ID=<uuid> \
#   OXY_TOKEN=<publish-scoped token> \
#     ./scripts/restatement-snapshot.sh [--days 35] [--out snapshots/restatement]
#
# Output: <out>/<UTC-date>.json — an object keyed by "<measure>@<granularity>",
# each mapping bucket start (ISO date) to the value observed *today*.

set -euo pipefail

DAYS=35
OUT_DIR="snapshots/restatement"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --days) DAYS="$2"; shift 2 ;;
    --out) OUT_DIR="$2"; shift 2 ;;
    -h|--help) sed -n '2,26p' "$0"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

: "${OXY_BASE_URL:?set OXY_BASE_URL (e.g. https://app.oxygen-hq.com)}"
: "${OXY_WORKSPACE_ID:?set OXY_WORKSPACE_ID}"
: "${OXY_TOKEN:?set OXY_TOKEN}"

command -v jq >/dev/null || { echo "jq is required" >&2; exit 1; }

API="${OXY_BASE_URL%/}/api/workspaces/${OXY_WORKSPACE_ID}"
AUTH=(-H "Authorization: Bearer ${OXY_TOKEN}" -H "Content-Type: application/json")

TODAY="$(date -u +%F)"
# The window is inclusive of today's (incomplete) bucket on purpose: a bucket's
# first observation is exactly the one that later restatements are measured
# against, and dropping it would lose the largest correction in the series.
FROM="$(date -u -d "${DAYS} days ago" +%F 2>/dev/null || date -u -v-"${DAYS}"d +%F)"

mkdir -p "${OUT_DIR}"
OUT="${OUT_DIR}/${TODAY}.json"

monitors="$(curl -fsS "${AUTH[@]}" "${API}/semantic/monitors")" || {
  echo "failed to list monitors — check OXY_BASE_URL / OXY_TOKEN / OXY_WORKSPACE_ID" >&2
  exit 1
}

# One query per (measure, time_dimension, granularity). Segments are collapsed
# deliberately: restatement is a property of the upstream load, not of a
# `group_by` slice, and the per-segment fan-out would multiply the query count
# by the store count for no extra signal.
echo "{}" > "${OUT}.tmp"
# A partial snapshot silently reads as restatement when the series is diffed, so
# the whole point of the script is defeated by a truncated day published as
# complete. The loop runs in a pipe subshell and can't set a parent variable, so
# any measure that fails touches this marker file; the final publish is gated on
# it being absent.
INCOMPLETE_MARKER="${OUT}.incomplete"
rm -f "${INCOMPLETE_MARKER}"
echo "${monitors}" | jq -c '
    .monitors
    | map({measure, time_dimension, granularity: (.granularity // "day")})
    | unique
    | .[]
  ' | while IFS= read -r monitor; do
  measure="$(jq -r '.measure' <<<"${monitor}")"
  time_dim="$(jq -r '.time_dimension' <<<"${monitor}")"
  gran="$(jq -r '.granularity' <<<"${monitor}")"
  key="${measure}@${gran}"

  body="$(jq -n \
    --arg measure "${measure}" \
    --arg dim "${time_dim}" \
    --arg gran "${gran}" \
    --arg from "${FROM}" \
    --arg to "${TODAY}" \
    '{
      measures: [$measure],
      time_dimensions: [{ dimension: $dim, granularity: $gran }],
      filters: [{ field: $dim, op: "in_date_range", values: [$from, $to] }],
      orders: [{ field: $dim, direction: "asc" }],
      limit: 400
    }')"

  rows="$(curl -fsS "${AUTH[@]}" -X POST "${API}/semantic" -d "${body}")" || {
    echo "warn: query failed for ${key}; snapshot will be marked incomplete" >&2
    : > "${INCOMPLETE_MARKER}"
    continue
  }

  # Bucket -> value. Connectors disagree on whether a column comes back fully
  # qualified (`sales.revenue`) or bare (`revenue`), so try both rather than
  # silently producing an empty series on the ones that answer differently.
  series="$(jq --arg dim "${time_dim}" --arg measure "${measure}" '
      (if type == "object" then (.data // .rows // []) else . end)
      | map({ (.[$dim] // .[($dim | split(".") | last)] | tostring):
              (.[$measure] // .[($measure | split(".") | last)]) })
      | add // {}
    ' <<<"${rows}")"

  if jq --arg key "${key}" --argjson series "${series}" \
       '.[$key] = $series' "${OUT}.tmp" > "${OUT}.tmp2"; then
    mv "${OUT}.tmp2" "${OUT}.tmp"
  else
    echo "warn: failed to merge ${key}; snapshot will be marked incomplete" >&2
    rm -f "${OUT}.tmp2"
    : > "${INCOMPLETE_MARKER}"
  fi
done

# Refuse to publish a truncated day under the canonical name — a missing measure
# would read as a restatement to the diff tooling.
if [[ -f "${INCOMPLETE_MARKER}" ]]; then
  rm -f "${INCOMPLETE_MARKER}" "${OUT}.tmp"
  echo "error: one or more measures failed; not publishing a partial snapshot for ${TODAY}" >&2
  exit 1
fi

mv "${OUT}.tmp" "${OUT}"
echo "wrote ${OUT} ($(jq 'keys | length' "${OUT}") measures, ${DAYS}d window)"
