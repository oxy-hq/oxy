#!/usr/bin/env bash
# Reached only when check-browser.sh finds the runner genuinely missing
# something. Everything here exists to survive a bad Ubuntu mirror; the
# strict re-check afterwards is what decides whether the job can proceed, so
# this script warns rather than fails.
set -uo pipefail

# apt ships no retry policy on these images, so the first 503 from an
# unhealthy mirror node is terminal. Give it one — `playwright install-deps`
# shells out to apt-get with flags we cannot reach, so it has to go in apt's
# own config.
sudo tee /etc/apt/apt.conf.d/99-playwright-retries > /dev/null << 'CONF'
Acquire::Retries "5";
Acquire::http::Timeout "30";
Acquire::https::Timeout "30";
CONF

# Move off the regional mirror before the first attempt, not after a failure.
# `us-east-2.ec2.ports.ubuntu.com` and `azure.archive.ubuntu.com` are DNS
# round-robins over many nodes, and an unhealthy member answers 503 or just
# hangs — which is exactly the failure that brought this matrix down.
# archive/ports.ubuntu.com sit behind Cloudflare instead.
case "$(dpkg --print-architecture)" in
  amd64 | i386) canonical="http://archive.ubuntu.com/ubuntu" ;;
  *) canonical="http://ports.ubuntu.com/ubuntu-ports" ;;
esac

echo "Pointing apt at ${canonical}"
for f in /etc/apt/sources.list /etc/apt/sources.list.d/*.list /etc/apt/sources.list.d/*.sources; do
  [ -f "$f" ] || continue
  # Only prefixed (regional) mirrors match — the canonical hosts have no
  # subdomain, so this is idempotent.
  sudo sed -i -E \
    "s#https?://[A-Za-z0-9.-]+\.(archive|ports)\.ubuntu\.com/(ubuntu-ports|ubuntu)#${canonical}#g" \
    "$f"
done

for attempt in 1 2 3; do
  if pnpm exec playwright install-deps chromium; then
    exit 0
  fi
  echo "::warning::playwright install-deps chromium failed (attempt ${attempt}/3)"
  sleep $((attempt * 5))
done

echo "::warning::Could not install Playwright system dependencies — every mirror attempt failed. Continuing; the strict Chromium check decides whether this run is viable."
exit 0
