#!/usr/bin/env bash
# Report whether any authored reference card under
# crates/agentic/builder/knowledge/ has drifted from its upstream source
# in oxy-hq/skills.
#
# Each *-reference.md card carries YAML frontmatter:
#
#   ---
#   source:
#     - oxy-hq/skills/skills/<skill>/SKILL.md
#     - oxy-hq/skills/skills/<skill>/QUICK-REFERENCE.md
#   reconciled-at: <skills-repo-commit-sha>
#   ---
#
# This script asks GitHub's compare endpoint for the file-level diff
# between `reconciled-at` and `main`, then reports any source file that
# appears in that diff. Ancestry is handled correctly: if every commit
# touching the source path is an ancestor of `reconciled-at`, the card
# is `current`; if any commit on main since `reconciled-at` modified the
# source, the card is `BEHIND`.
#
# Exit code:
#   0 if every card is current,
#   1 if any card is behind (drift detected),
#   2 on infrastructure / parsing failure.
#
# Optional env:
#   GH_TOKEN          - GitHub token used for the API call. Avoids the
#                       60 req/hr unauth rate limit. CI sets this.
#   SKILLS_REPO       - "owner/repo" override (default: oxy-hq/skills).
#   SKILLS_REF        - branch / sha to compare against (default: main).
#   KNOWLEDGE_DIR     - cards directory override (default:
#                       crates/agentic/builder/knowledge).

set -euo pipefail

SKILLS_REPO="${SKILLS_REPO:-oxy-hq/skills}"
SKILLS_REF="${SKILLS_REF:-main}"
KNOWLEDGE_DIR="${KNOWLEDGE_DIR:-crates/agentic/builder/knowledge}"

if [ ! -d "$KNOWLEDGE_DIR" ]; then
  echo "Knowledge directory not found at $KNOWLEDGE_DIR" >&2
  echo "Run this script from the oxygen-internal repo root." >&2
  exit 2
fi

if ! command -v curl >/dev/null 2>&1; then
  echo "curl is required" >&2
  exit 2
fi

api_call () {
  local path="$1"
  local args=(-fsSL)
  if [ -n "${GH_TOKEN:-}" ]; then
    args+=(-H "Authorization: Bearer ${GH_TOKEN}")
  fi
  args+=(-H "Accept: application/vnd.github+json")
  args+=(-H "X-GitHub-Api-Version: 2022-11-28")
  curl "${args[@]}" "https://api.github.com/repos/${SKILLS_REPO}/${path}"
}

# Extract the YAML frontmatter block at the top of a markdown file.
# Frontmatter must start on line 1 with `---` and end with another
# `---` line.
read_frontmatter () {
  awk '
    NR == 1 && $0 != "---" { exit 1 }
    NR == 1 { in_fm = 1; next }
    in_fm && $0 == "---" { exit 0 }
    in_fm { print }
  ' "$1"
}

field_scalar () {
  awk -v key="$2" '
    $1 == key":" { sub(/^[^:]+:[ \t]*/, ""); print; exit }
  ' <<< "$1"
}

field_list () {
  awk -v key="$2" '
    $1 == key":" { in_list = 1; next }
    in_list && $0 ~ /^[ \t]+-[ \t]+/ { sub(/^[ \t]+-[ \t]+/, ""); print; next }
    in_list { in_list = 0 }
  ' <<< "$1"
}

# Print one row of the report.
row () {
  printf "%-58s %-10s %-10s %s\n" "$1" "$2" "$3" "$4"
}

drift_count=0
total_count=0

# Memoize the most recent compare response. Cards reconciled at the same
# upstream SHA (the common case) reuse the previous lookup. Implemented
# with simple variables so the script runs on bash 3.x (macOS default)
# without needing `declare -A`.
last_pinned=""
last_changed=""

row "card / source" "pinned" "head" "status"
row "------------" "------" "----" "------"

for card in "$KNOWLEDGE_DIR"/*-reference.md; do
  [ -e "$card" ] || continue

  fm=$(read_frontmatter "$card") || {
    echo "MISSING FRONTMATTER: $card" >&2
    exit 2
  }

  pinned=$(field_scalar "$fm" "reconciled-at")
  if [ -z "$pinned" ]; then
    echo "MISSING reconciled-at IN: $card" >&2
    exit 2
  fi

  # Fetch the diff lazily. If consecutive cards share the same pinned
  # SHA (common when bumping the whole knowledge module) we reuse the
  # previous result.
  if [ "$pinned" != "$last_pinned" ]; then
    if resp=$(api_call "compare/${pinned}...${SKILLS_REF}" 2>/dev/null); then
      # When the compare diff is empty (HEAD == base) `grep -oE` finds
      # no matches and exits 1, which kills the script under `set -e`.
      # Absorb that here so an empty diff yields an empty list instead.
      last_changed=$(printf "%s" "$resp" \
        | { grep -oE '"filename":[[:space:]]*"[^"]*"' || true; } \
        | sed -E 's/.*"filename":[[:space:]]*"([^"]+)".*/\1/')
    else
      last_changed="__API_ERROR__"
    fi
    last_pinned="$pinned"
  fi
  changed="$last_changed"

  while IFS= read -r src; do
    [ -z "$src" ] && continue
    total_count=$((total_count + 1))

    expected_prefix="${SKILLS_REPO}/"
    if [[ "$src" != "$expected_prefix"* ]]; then
      row "$(basename "$card") <- $src" "${pinned:0:8}" "?" \
        "BAD-PATH (expected $expected_prefix prefix)"
      drift_count=$((drift_count + 1))
      continue
    fi
    repo_path="${src#${expected_prefix}}"

    if [ "$changed" = "__API_ERROR__" ]; then
      row "$(basename "$card") <- $repo_path" "${pinned:0:8}" "?" "API-ERROR"
      drift_count=$((drift_count + 1))
      continue
    fi

    if printf "%s\n" "$changed" | grep -qFx "$repo_path"; then
      row "$(basename "$card") <- $repo_path" "${pinned:0:8}" \
        "$SKILLS_REF" "BEHIND"
      drift_count=$((drift_count + 1))
    else
      row "$(basename "$card") <- $repo_path" "${pinned:0:8}" \
        "$SKILLS_REF" "current"
    fi
  done < <(field_list "$fm" "source")
done

echo
if [ "$total_count" -eq 0 ]; then
  echo "No source entries found across $KNOWLEDGE_DIR/*-reference.md." >&2
  exit 2
fi

if [ "$drift_count" -gt 0 ]; then
  echo "Drift detected on $drift_count of $total_count source(s). Re-condense the affected card(s)"
  echo "from upstream and bump 'reconciled-at:' to a more recent SHA on ${SKILLS_REPO}@${SKILLS_REF}."
  exit 1
fi

echo "All $total_count card source(s) are current against ${SKILLS_REPO}@${SKILLS_REF}."
exit 0
