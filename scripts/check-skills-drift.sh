#!/usr/bin/env bash
# Report whether the builder knowledge under crates/agentic/builder/knowledge/
# has drifted from the platform skills it is condensed from.
#
# The sources live in this repo, in sdk/cli/skills/ — the same files that are
# published to oxy-hq/skills — so drift is a local comparison, with no API call
# and no history needed:
#
#   *-reference.md   an authored card. Its frontmatter lists `source:` paths and
#                    records `reconciled-hash:`, the git blob hash of those
#                    sources concatenated in order. A different hash now means a
#                    source changed since the card was last re-condensed: BEHIND.
#   *.yml            a verbatim template. Its `# Source:` header names the file;
#                    everything after the header must match it byte for byte.
#
# Usage:
#   scripts/check-skills-drift.sh                  status table
#   scripts/check-skills-drift.sh --hash <card>    the reconciled-hash to record
#                                                  after re-condensing <card>
#
# Exit code: 0 every card current, 1 drift, 2 setup or parse failure.
# Optional env: KNOWLEDGE_DIR (default crates/agentic/builder/knowledge).

set -euo pipefail

KNOWLEDGE_DIR="${KNOWLEDGE_DIR:-crates/agentic/builder/knowledge}"

if [ ! -d "$KNOWLEDGE_DIR" ]; then
  echo "Knowledge directory not found at $KNOWLEDGE_DIR" >&2
  echo "Run this script from the oxygen-internal repo root." >&2
  exit 2
fi

# Print the YAML frontmatter body (between the leading `---` fences).
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

# The hash of a card's sources, concatenated in the order the card lists them.
sources_hash () {
  local card="$1" fm src
  local files=()
  fm=$(read_frontmatter "$card") || { echo "MISSING FRONTMATTER: $card" >&2; return 2; }
  while IFS= read -r src; do
    [ -z "$src" ] && continue
    if [ ! -f "$src" ]; then
      echo "MISSING SOURCE: $src (listed by $card)" >&2
      return 2
    fi
    files+=("$src")
  done < <(field_list "$fm" "source")
  if [ "${#files[@]}" -eq 0 ]; then
    echo "NO source: ENTRIES IN: $card" >&2
    return 2
  fi
  cat "${files[@]}" | git hash-object --stdin
}

if [ "${1:-}" = "--hash" ]; then
  [ -n "${2:-}" ] || { echo "usage: $0 --hash <card>" >&2; exit 2; }
  sources_hash "$2"
  exit
fi

row () {
  printf "%-46s %-62s %s\n" "$1" "$2" "$3"
}

drift=0
checked=0
row "card" "source" "status"
row "----" "------" "------"

for card in "$KNOWLEDGE_DIR"/*-reference.md; do
  [ -e "$card" ] || continue
  fm=$(read_frontmatter "$card") || { echo "MISSING FRONTMATTER: $card" >&2; exit 2; }
  recorded=$(field_scalar "$fm" "reconciled-hash")
  now=$(sources_hash "$card") || exit 2
  checked=$((checked + 1))
  status="current"
  if [ "$recorded" != "$now" ]; then
    status="BEHIND (re-condense, then reconciled-hash: $now)"
    drift=$((drift + 1))
  fi
  row "$(basename "$card")" "$(field_list "$fm" "source" | head -1)" "$status"
done

for template in "$KNOWLEDGE_DIR"/*.yml; do
  [ -e "$template" ] || continue
  src=$(awk '/^# Source: / { sub(/^# Source: /, ""); print; exit } /^$/ { exit }' "$template")
  if [ -z "$src" ] || [ ! -f "$src" ]; then
    echo "TEMPLATE WITHOUT A READABLE '# Source:' HEADER: $template (${src:-none})" >&2
    exit 2
  fi
  checked=$((checked + 1))
  # The body is everything after the header's first blank line — the same
  # boundary `strip_vendoring_header` in crates/agentic/builder/src/prompts.rs uses.
  if awk 'body { print; next } /^$/ { body = 1 }' "$template" | cmp -s - "$src"; then
    row "$(basename "$template")" "$src" "current"
  else
    row "$(basename "$template")" "$src" "BEHIND (run scripts/sync-skills.sh)"
    drift=$((drift + 1))
  fi
done

echo
if [ "$checked" -eq 0 ]; then
  echo "Nothing to check under $KNOWLEDGE_DIR." >&2
  exit 2
fi
if [ "$drift" -gt 0 ]; then
  echo "Drift on $drift of $checked file(s). Re-condense each BEHIND card from its sources and"
  echo "record the printed reconciled-hash; refresh templates with scripts/sync-skills.sh."
  exit 1
fi
echo "All $checked knowledge file(s) are current with sdk/cli/skills."
