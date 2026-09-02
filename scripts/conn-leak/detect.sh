#!/usr/bin/env bash
#
# Run scripts/conn-leak/detect.sql against an Oxy deployment.
#
# The SQL is the artifact; this only solves "how do I get a psql to that
# cluster's Postgres". Two ways in:
#
#   ./detect.sh --url "postgres://user:pass@host:5432/db"      # direct
#   ./detect.sh --context oxy-dev --ns postgres \
#               --pod oxy-dev-postgres-1 --user oxy --db oxydb # via kubectl exec
#
# The kubectl form execs psql INSIDE the postgres pod over 127.0.0.1, because
# peer auth rejects the `oxy` role on the unix socket. PGPASSWORD is read from
# $PGPASSWORD, or --password.
#
# Read-only: detect.sql contains only SELECTs and one CREATE TEMP VIEW.
set -euo pipefail

SQL_FILE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/detect.sql"

URL="" CONTEXT="" NS="postgres" POD="" PGUSER_="oxy" PGDB="oxydb" PGPASS="${PGPASSWORD:-}"
while [[ $# -gt 0 ]]; do
  case "$1" in
    --url)      URL="$2"; shift 2 ;;
    --context)  CONTEXT="$2"; shift 2 ;;
    --ns)       NS="$2"; shift 2 ;;
    --pod)      POD="$2"; shift 2 ;;
    --user)     PGUSER_="$2"; shift 2 ;;
    --db)       PGDB="$2"; shift 2 ;;
    --password) PGPASS="$2"; shift 2 ;;
    -h|--help)  sed -n '2,20p' "${BASH_SOURCE[0]}"; exit 0 ;;
    *) echo "unknown flag: $1" >&2; exit 2 ;;
  esac
done

if [[ -n "$URL" ]]; then
  exec psql "$URL" -v ON_ERROR_STOP=1 -f "$SQL_FILE"
fi

if [[ -z "$POD" ]]; then
  echo "need --url, or --pod (with optional --context/--ns/--user/--db)" >&2
  exit 2
fi

kube=(kubectl)
[[ -n "$CONTEXT" ]] && kube+=(--context "$CONTEXT")

# -i so the heredoc'd SQL reaches psql's stdin; `-f -` makes psql read it.
"${kube[@]}" exec -i "$POD" -n "$NS" -c postgres -- \
  sh -c "PGPASSWORD='${PGPASS}' psql -h 127.0.0.1 -U '${PGUSER_}' -d '${PGDB}' -v ON_ERROR_STOP=1 -f -" \
  < "$SQL_FILE"
