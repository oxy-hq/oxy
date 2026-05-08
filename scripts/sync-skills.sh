#!/usr/bin/env bash
# Sync vendored reference docs from ../skills into the builder crate.
# Run from the oxygen-internal repo root:
#
#     ./scripts/sync-skills.sh
#
# The script copies the verbatim YAML templates into
# crates/agentic/builder/knowledge/ and stamps the current skills-repo
# commit into the `Last synced:` line of knowledge/README.md.
#
# Authored reference docs (*-reference.md) are maintained by hand and are
# NOT overwritten here — re-condense them when the source material
# changes materially. Drift between cards and upstream is reported by
# scripts/check-skills-drift.sh.
#
# Override the skills checkout location with SKILLS_DIR=/path/to/skills.
# Pin the sync to a specific skills-repo commit with SKILLS_PIN=<sha>.
# When SKILLS_PIN is set, the script briefly checks that commit out in
# SKILLS_DIR, vendors from it, then restores the original HEAD.

set -euo pipefail

SKILLS_DIR="${SKILLS_DIR:-../skills}"
SKILLS_PIN="${SKILLS_PIN:-}"
DEST="crates/agentic/builder/knowledge"

if [ ! -d "$SKILLS_DIR" ]; then
  echo "Skills repo not found at $SKILLS_DIR" >&2
  echo "Set SKILLS_DIR to the path of your oxy-hq/skills checkout." >&2
  exit 1
fi

if [ ! -d "$DEST" ]; then
  echo "Knowledge destination not found at $DEST" >&2
  echo "Run this script from the oxygen-internal repo root." >&2
  exit 1
fi

# Optional: pin the skills checkout to a specific commit for reproducible
# vendoring. The original HEAD is restored on exit (success or failure)
# so an interrupted sync doesn't strand the user's checkout on the pin.
if [ -n "$SKILLS_PIN" ]; then
  if ! ORIGINAL_REF=$(git -C "$SKILLS_DIR" symbolic-ref --quiet --short HEAD 2>/dev/null); then
    ORIGINAL_REF=$(git -C "$SKILLS_DIR" rev-parse HEAD)
  fi

  if ! git -C "$SKILLS_DIR" diff --quiet || ! git -C "$SKILLS_DIR" diff --cached --quiet; then
    echo "Skills repo at $SKILLS_DIR has uncommitted changes; refusing to switch." >&2
    echo "Commit, stash, or discard them before running with SKILLS_PIN." >&2
    exit 1
  fi

  restore_skills_head () {
    git -C "$SKILLS_DIR" checkout --quiet "$ORIGINAL_REF" || true
  }
  trap restore_skills_head EXIT

  echo "Pinning skills checkout to ${SKILLS_PIN} (was ${ORIGINAL_REF})."
  git -C "$SKILLS_DIR" checkout --quiet "$SKILLS_PIN"
fi

# Copy verbatim YAML templates. The header on each file points back at
# the skills repo so readers know where to go to edit them.
copy_verbatim () {
  local src="$1"
  local dst="$2"
  local rel_src="$3"

  if [ ! -f "$src" ]; then
    echo "Missing source: $src" >&2
    exit 1
  fi

  if [ ! -s "$src" ]; then
    echo "Source file is empty: $src" >&2
    echo "Refusing to overwrite $dst with an empty file." >&2
    exit 1
  fi

  {
    echo "# Vendored from oxy-hq/skills @ ${SKILLS_COMMIT_SHORT}"
    echo "# Source: $rel_src"
    echo "# Do not edit by hand — edit the skills repo and re-run scripts/sync-skills.sh."
    echo
    cat "$src"
  } > "$dst"
  echo "  synced: $dst"
}

SKILLS_COMMIT=$(git -C "$SKILLS_DIR" rev-parse HEAD)
SKILLS_COMMIT_SHORT=$(git -C "$SKILLS_DIR" rev-parse --short HEAD)

copy_verbatim \
  "$SKILLS_DIR/skills/oxy-semantic-layer/view-template.yml" \
  "$DEST/view-template.yml" \
  "skills/oxy-semantic-layer/view-template.yml"

copy_verbatim \
  "$SKILLS_DIR/skills/oxy-semantic-layer/topic-template.yml" \
  "$DEST/topic-template.yml" \
  "skills/oxy-semantic-layer/topic-template.yml"

copy_verbatim \
  "$SKILLS_DIR/skills/oxy-agentic-builder/agentic-template.yml" \
  "$DEST/agentic-template.yml" \
  "skills/oxy-agentic-builder/agentic-template.yml"

# Update the "Last synced:" line in the knowledge README.
sed -i.bak -E "s|^Last synced: .*|Last synced: skills@${SKILLS_COMMIT}|" "$DEST/README.md"
rm -f "$DEST/README.md.bak"

echo
echo "Synced verbatim files and stamped skills@${SKILLS_COMMIT_SHORT}."
echo "Authored reference docs were not touched; re-condense by hand if the"
echo "skills sources changed materially."
echo
echo "Review the diff with:"
echo "  git diff -- $DEST"
