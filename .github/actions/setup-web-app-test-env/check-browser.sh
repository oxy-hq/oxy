#!/usr/bin/env bash
# Decide whether this runner actually needs an apt round-trip before it can
# drive Chromium — and, with --strict, fail the job if it still cannot.
#
# The runner images already carry every shared library Chromium links against;
# what `playwright install-deps` adds on top is fonts. So the question worth
# asking is not "did apt succeed" but "can Chromium launch and render text".
# Answering it here is what lets the normal path skip apt entirely, and with
# it the Ubuntu mirror that keeps 503-ing (see action.yaml).
set -uo pipefail

strict=false
[ "${1:-}" = "--strict" ] && strict=true

reasons=()
tmp="${RUNNER_TEMP:-/tmp}"

if pnpm exec playwright screenshot --browser=chromium about:blank \
  "${tmp}/chromium-launch-check.png" > "${tmp}/chromium-launch-check.log" 2>&1; then
  echo "Chromium launches."
else
  echo "Chromium failed to launch:"
  tail -n 30 "${tmp}/chromium-launch-check.log"
  reasons+=("chromium-launch")
fi

# Fonts don't gate launching, only legible rendering — and the agentic flows
# feed screenshots to an LLM, so a box of tofu would fail as a confusing
# behavioral error rather than a setup one.
if ! command -v fc-list > /dev/null 2>&1; then
  echo "fontconfig is not installed."
  reasons+=("fontconfig")
elif [ "$(fc-list :lang=en family 2> /dev/null | wc -l)" -eq 0 ]; then
  echo "No Latin-capable font is installed."
  reasons+=("fonts")
else
  echo "Fonts: $(fc-list :lang=en family | wc -l) Latin-capable families installed."
fi

out="${GITHUB_OUTPUT:-/dev/null}"

if [ ${#reasons[@]} -eq 0 ]; then
  echo "Runner can drive Chromium as-is; skipping apt."
  echo "needs-repair=false" >> "$out"
  exit 0
fi

echo "needs-repair=true" >> "$out"
echo "reasons=${reasons[*]}" >> "$out"

if [ "$strict" = false ]; then
  echo "Missing: ${reasons[*]} — will try to repair via apt."
  exit 0
fi

case " ${reasons[*]} " in
  *" chromium-launch "*)
    echo "::error::Chromium still cannot launch after installing system dependencies (missing: ${reasons[*]}). This is a real dependency gap, not the usual mirror flake."
    exit 1
    ;;
esac

echo "::warning::Chromium runs, but ${reasons[*]} could not be installed — text may render with fallback glyphs."
exit 0
