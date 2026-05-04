#!/usr/bin/env bash
# Context: the project is being rebranded from "oxy" to "oxygen", but the
# existing GHCR images are published as `ghcr.io/oxy-hq/oxy[-semantic-engine]`.
# This one-shot script publishes `oxygen[-semantic-engine]` aliases pointing
# at the same image manifests so existing users can switch to the new name
# without us re-releasing every version.
#
# Mirrors the 10 most recent stable (semver) and 10 most recent edge-dated tags
# from `oxy*` container packages to `oxygen*` aliases on ghcr.io/oxy-hq.
#
# Uses `crane copy` so layers are not re-uploaded — only the manifest is
# copied (cross-repo blob mount on the registry side). Arch-specific variants
# (-amd64, -arm64) are mirrored alongside the multi-arch tag when they exist.
#
# Requirements:
#   - gh (authenticated, with read:packages on oxy-hq)
#   - crane (`brew install crane`), authenticated to ghcr.io with a token that
#     has write:packages on oxy-hq, e.g.:
#       gh auth refresh -h github.com -s write:packages
#       gh auth token | crane auth login ghcr.io -u "$(gh api user -q .login)" --password-stdin
#
# Usage: ./scripts/mirror-oxy-to-oxygen.sh [--dry-run]

set -euo pipefail

REGISTRY="ghcr.io/oxy-hq"
RECENT_COUNT=10
DRY_RUN=0
[[ "${1:-}" == "--dry-run" ]] && DRY_RUN=1

# source -> destination package name
declare -a MAPPINGS=(
  "oxy:oxygen"
  "oxy-semantic-engine:oxygen-semantic-engine"
)

run() {
  if (( DRY_RUN )); then
    echo "DRY: $*"
  else
    echo "RUN: $*"
    "$@"
  fi
}

list_tags() {
  local pkg="$1"
  gh api --paginate "/orgs/oxy-hq/packages/container/${pkg}/versions" \
    --jq '.[].metadata.container.tags[]' 2>/dev/null | sort -u
}

# Pick the 10 most recent stable tags (X.Y.Z), sorted by semver descending.
select_stable() {
  grep -E '^[0-9]+\.[0-9]+\.[0-9]+$' | sort -uV | tail -n "$RECENT_COUNT"
}

# Pick the 10 most recent edge-dated tags (edge-YYYYMMDD), lexical descending.
select_edge() {
  grep -E '^edge-[0-9]{8}$' | sort -u | tail -n "$RECENT_COUNT"
}

mirror_tag() {
  local src_pkg="$1" dst_pkg="$2" tag="$3" all_tags="$4"
  run crane copy "${REGISTRY}/${src_pkg}:${tag}" "${REGISTRY}/${dst_pkg}:${tag}"

  # Mirror arch variants if they exist on the source.
  for arch in amd64 arm64; do
    local variant="${tag}-${arch}"
    if grep -qx "$variant" <<<"$all_tags"; then
      run crane copy \
        "${REGISTRY}/${src_pkg}:${variant}" \
        "${REGISTRY}/${dst_pkg}:${variant}"
    fi
  done
}

for pair in "${MAPPINGS[@]}"; do
  src_pkg="${pair%%:*}"
  dst_pkg="${pair##*:}"

  echo
  echo "==> Mirroring ${src_pkg} -> ${dst_pkg}"

  all_tags="$(list_tags "$src_pkg")"

  stable_tags="$(echo "$all_tags" | select_stable || true)"
  edge_tags="$(echo "$all_tags" | select_edge || true)"

  echo "    stable: $(echo "$stable_tags" | tr '\n' ' ')"
  echo "    edge:   $(echo "$edge_tags" | tr '\n' ' ')"

  while IFS= read -r tag; do
    [[ -z "$tag" ]] && continue
    mirror_tag "$src_pkg" "$dst_pkg" "$tag" "$all_tags"
  done < <(printf '%s\n%s\n' "$stable_tags" "$edge_tags")
done

echo
echo "Done. Set the new packages public (one-time) with:"
for pair in "${MAPPINGS[@]}"; do
  dst_pkg="${pair##*:}"
  echo "  gh api -X PATCH /orgs/oxy-hq/packages/container/${dst_pkg} --field visibility=public"
done
