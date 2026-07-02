#!/usr/bin/env bash
# airhouse: mint a TEMPORARY pgwire user, psql in, and AUTO-REVOKE the temp
# service account on exit. Assumes the matching port-forwards are running:
#     ./port-forward.sh [dev|prod]
#
# Usage:  bash connect_airhouse.sh [dev|prod] [tenant] [role] [ttl_secs]
#   env       default: dev    (prod must be given explicitly)
#   tenant    default: dev -> oxy-dev-observability ; prod -> oxy-observability
#   role      default: reader (use 'admin' for DDL like SET SORTED BY / writes)
#   ttl_secs  default: 3600   (system max 86400)
#
# Cleanup: the temp service account is DELETED automatically when you exit psql
# (trap on EXIT — works because we don't `exec`, so the admin token is still in
# scope). Set KEEP=1 to leave the credential alive until its TTL; the script then
# prints a self-contained revoke command (re-reads the admin token, no stale var).
#
# Prereq: aws sso login --profile oxy-<env>
set -uo pipefail

ENV="${1:-dev}"
case "$ENV" in
  dev)  CTX=oxy-dev;  NS=airhouse-dev; PROFILE=oxy-dev;  CP_PORT=19090; PG_PORT=25446; OBS=oxy-dev-observability ;;
  prod) CTX=oxy-prod; NS=airhouse;     PROFILE=oxy-prod; CP_PORT=19190; PG_PORT=25546; OBS=oxy-observability ;;
  *) echo "FAIL: env must be 'dev' or 'prod' (got '$ENV')"; exit 1 ;;
esac
TENANT="${2:-$OBS}"; ROLE="${3:-reader}"; TTL="${4:-3600}"
ADMIN_SECRET="$NS-admin"
HTTP="http://127.0.0.1:$CP_PORT"

command -v jq >/dev/null || { echo "FAIL: need jq"; exit 1; }

# ── preflight: are the port-forwards up for THIS env? ────────────────────────
if ! curl -sf -m 3 "$HTTP/healthz" >/dev/null 2>&1; then
  cat >&2 <<EOF
FAIL: $ENV control-plane not reachable on :$CP_PORT. Start the port-forwards first
(in another terminal, keep them running):

  ./port-forward.sh $ENV

(If kubectl itself errors: aws sso login --profile $PROFILE)
EOF
  exit 1
fi
[ "$ENV" = prod ] && echo "⚠️  PROD ($CTX/$NS) — role=$ROLE. Avoid DDL/writes against prod tenants." >&2

# ── 1. retrieve CP admin token from the k8s secret ───────────────────────────
echo "→ [$ENV] retrieving CP admin token from secret $ADMIN_SECRET"
ADMIN_TOKEN=$(kubectl --context "$CTX" -n "$NS" get secret "$ADMIN_SECRET" -o jsonpath='{.data.token}' \
  | python3 -c 'import sys,base64;sys.stdout.write(base64.b64decode(sys.stdin.read()).decode())') \
  || { echo "FAIL: get-secret $ADMIN_SECRET (SSO expired? run: aws sso login --profile $PROFILE)"; exit 1; }
[ -z "$ADMIN_TOKEN" ] && { echo "FAIL: empty admin token from $ADMIN_SECRET"; exit 1; }

# ── 2. service account (capped at ROLE), with auto-revoke on exit ────────────
echo "→ [$ENV] minting $ROLE credential for tenant '$TENANT' (ttl ${TTL}s)"
SA=$(curl -s -X POST "$HTTP/admin/v1/service-accounts" \
  -H "Authorization: Bearer $ADMIN_TOKEN" -H "Content-Type: application/json" \
  -d "{\"name\":\"$ROLE-$TENANT\",\"tenant_id\":\"$TENANT\",\"max_role\":\"$ROLE\",\"max_ttl_secs\":$TTL}")
SA_BEARER=$(echo "$SA" | jq -r '.bearer // empty'); SA_ID=$(echo "$SA" | jq -r '.id // empty')
[ -z "$SA_BEARER" ] && { echo "FAIL creating service account: $SA"; exit 1; }

revoke() { [ -n "${SA_ID:-}" ] || return 0
  curl -s -X DELETE "$HTTP/admin/v1/service-accounts/$SA_ID" \
    -H "Authorization: Bearer $ADMIN_TOKEN" -o /dev/null && echo "✓ revoked temp SA $SA_ID" >&2; }
AUTO=1; [ "${KEEP:-0}" = 1 ] && AUTO=0; command -v psql >/dev/null || AUTO=0
[ "$AUTO" = 1 ] && trap revoke EXIT

# ── 3. mint the short-lived token ────────────────────────────────────────────
MINT=$(curl -s -X POST "$HTTP/admin/v1/tenants/$TENANT/tokens" \
  -H "Authorization: Bearer $SA_BEARER" -H "Content-Type: application/json" \
  -d "{\"subject\":\"$ROLE\",\"role\":\"$ROLE\",\"ttl_secs\":$TTL}")
USER=$(echo "$MINT" | jq -r '.username // empty'); PASS=$(echo "$MINT" | jq -r '.password // empty')
EXP=$(echo "$MINT" | jq -r '.expires_at // empty')
[ -z "$USER" ] && { revoke; echo "FAIL minting token: $MINT"; exit 1; }

URL="postgres://$USER:$PASS@127.0.0.1:$PG_PORT/$TENANT"
echo
echo "  [$ENV] connection ($ROLE, expires $EXP):"
echo "    $URL"
if [ "$AUTO" = 1 ]; then
  echo "  (temp service account $SA_ID auto-revokes when you exit psql)"
else
  echo "  credential left alive (KEEP/no-psql). Revoke the temp SA with:"
  echo "    kubectl --context $CTX -n $NS get secret $ADMIN_SECRET -o jsonpath='{.data.token}' | base64 -d \\"
  echo "      | xargs -I{} curl -s -X DELETE $HTTP/admin/v1/service-accounts/$SA_ID -H 'Authorization: Bearer {}'"
fi
echo

# ── 4. psql (NOT exec — so the EXIT trap can revoke afterward) ────────────────
if command -v psql >/dev/null; then psql "$URL"; else echo "(psql not found — use the URL above)"; fi
