#!/usr/bin/env bash
# Start airhouse port-forwards (control-plane + pgwire) for an environment, on
# collision-free local ports. Foreground — Ctrl-C stops both. Keep it running in
# its own terminal; use connect_airhouse.sh from another.
#
# Usage:  ./port-forward.sh [dev|prod]      (default: dev; prod must be explicit)
# Prereq: aws sso login --profile oxy-<env>
#
# Local ports differ per env so dev and prod tunnels can coexist and a dev
# session can never accidentally hit a prod forward:
#   dev :  CP 19090  serving 25445  analytics 25446
#   prod:  CP 19190  serving 25545  analytics 25546
set -uo pipefail

ENV="${1:-dev}"
case "$ENV" in
  dev)  CTX=oxy-dev;  NS=airhouse-dev; PROFILE=oxy-dev;  CP=19090; SERVING=25445; ANALYTICS=25446 ;;
  prod) CTX=oxy-prod; NS=airhouse;     PROFILE=oxy-prod; CP=19190; SERVING=25545; ANALYTICS=25546 ;;
  *) echo "FAIL: env must be 'dev' or 'prod' (got '$ENV')"; exit 1 ;;
esac

kubectl --context "$CTX" -n "$NS" get ns "$NS" >/dev/null 2>&1 \
  || { echo "kubectl can't reach $CTX — run: aws sso login --profile $PROFILE"; exit 1; }

[ "$ENV" = prod ] && echo "⚠️  PROD port-forward ($CTX/$NS)"
echo "port-forward [$ENV] → CP :$CP | pgwire :$SERVING (serving) :$ANALYTICS (analytics)"
echo "Ctrl-C to stop both."
trap 'kill 0' EXIT
kubectl --context "$CTX" -n "$NS" port-forward --address 127.0.0.1 "svc/$NS-cp" "$CP:8080" &
kubectl --context "$CTX" -n "$NS" port-forward --address 127.0.0.1 "svc/$NS-haproxy" "$SERVING:5445" "$ANALYTICS:5446" &
wait
