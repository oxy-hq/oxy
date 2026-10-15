#!/usr/bin/env bash
# Copy the verbatim YAML templates the builder embeds out of the platform skills.
#
# The templates' source of truth is sdk/cli/skills/ (published to oxy-hq/skills).
# The builder cannot `include_str!` them from there — the crate would then depend
# on a path outside it — so they are copied in with a vendoring header that
# `strip_vendoring_header` (crates/agentic/builder/src/prompts.rs) removes before
# the model sees them. The header must start with `# Vendored from` and end at
# its first blank line; the stripper relies on both.
#
# Authored cards (*-reference.md) are NOT touched: they are condensed by hand or
# by the skills-reconcile workflow. scripts/check-skills-drift.sh reports both.
#
# Run from the oxygen-internal repo root.

set -euo pipefail

DEST="crates/agentic/builder/knowledge"
SKILLS="sdk/cli/skills"

if [ ! -d "$DEST" ] || [ ! -d "$SKILLS" ]; then
  echo "Run this script from the oxygen-internal repo root." >&2
  exit 1
fi

copy_verbatim () {
  local src="$SKILLS/$1"
  local dst="$DEST/$2"
  if [ ! -s "$src" ]; then
    echo "Missing or empty source: $src" >&2
    exit 1
  fi
  {
    echo "# Vendored from $SKILLS — do not edit here; edit the skill and run scripts/sync-skills.sh."
    echo "# Source: $src"
    echo
    cat "$src"
  } > "$dst"
  echo "  synced: $dst"
}

copy_verbatim oxy-semantic-modeling/templates/view-template.yml view-template.yml
copy_verbatim oxy-semantic-modeling/templates/topic-template.yml topic-template.yml
copy_verbatim oxy-agents/templates/agentic-template.yml agentic-template.yml
