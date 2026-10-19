#!/usr/bin/env bash
# Print the runner's memory high-water mark (RAM + swap in use) each time it
# rises by STEP MiB, so a job the runner kills mid-build still leaves its last
# reading in the log. Run it in the background of the step you want covered;
# the step's own output is the only place the readings land.
#
#   scripts/ci/mem-watermark.sh [STEP_MIB=512] [INTERVAL_S=10] &
set -u

step=${1:-512}
interval=${2:-10}
peak=0

while sleep "$interval"; do
  read -r total avail swap_total swap_free < <(awk '
    /^MemTotal:/     { t = $2 }
    /^MemAvailable:/ { a = $2 }
    /^SwapTotal:/    { st = $2 }
    /^SwapFree:/     { sf = $2 }
    END { print t, a, st, sf }' /proc/meminfo)
  ram=$(((total - avail) / 1024))
  swap=$(((swap_total - swap_free) / 1024))
  used=$((ram + swap))
  if ((used >= peak + step)); then
    peak=$used
    echo "mem-watermark: ${used} MiB in use (RAM ${ram}/$((total / 1024)) MiB, swap ${swap}/$((swap_total / 1024)) MiB)"
  fi
done
