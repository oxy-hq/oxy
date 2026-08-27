#!/usr/bin/env bash
#
# Stand up ClickHouse, wire a server to it, and put enough spans in it for the
# observability flows to have something real to render.
#
# There is no seed path for observability data — `tests/agentic/fixtures/reset.ts`
# is deliberately a three-command surface that cannot reach a database, and the
# flows were authored against traces produced by paying for two live agent runs.
# That made them unrunnable in an automated pass: without data the pages render
# "Failed to load cluster data" and the flows fail for a reason that has nothing
# to do with the product.
#
# The spans below are inserted directly, the shape `just clickhouse-obs-verify`
# already uses. Deterministic, free, and the same rows every run — which is what
# a regression suite wants anyway. A trace id belongs to the ClickHouse instance
# that produced it, so nothing here is referenced by id from a flow; the flows
# open whichever trace the list is showing.
set -euo pipefail
cd "$(dirname "$0")/.."

# TWO ways to get a ClickHouse-backed server, and only one of them works on a
# machine that already runs anything else:
#
#   `oxy start --enterprise` boots its OWN `oxy-clickhouse`, binding host :8123
#   AND :9000. The Airhouse stack's MinIO also binds :9000, so on any box where
#   that is up, `oxy start` dies with "port is already allocated" before it ever
#   reaches the schema.
#
#   `oxy serve --enterprise` + `docker-compose.clickhouse.yml` connects to a
#   ClickHouse that maps native TCP to :9010 precisely to dodge that clash (see
#   the port comment in that compose file). This uses that one.
ch() { docker compose -f docker-compose.clickhouse.yml exec -T clickhouse clickhouse-client --password "${OXY_CLICKHOUSE_PASSWORD:-oxy-local}" "$@"; }

OWNER=$(grep -E '^OXY_OWNER=' .env 2>/dev/null | head -1 | cut -d= -f2- | tr -d '"'"'"'' || true)
# Two different Postgres setups live in this repo and they are easy to confuse:
#
#   `docker-compose.yml`  → container `postgres`, host :5432, admin/password,
#                           database `default`. This is what `.env`'s
#                           OXY_DATABASE_URL names, and what `oxy serve` wants.
#   `oxy start`           → container `oxy-postgres`, host :15432, database
#                           `oxy`, managed by the CLI, and its startup CLEANS UP
#                           existing containers — including the one above.
#
# So `.env` is right and the failure mode is simply that nothing is up: an
# absent container produces "pool timed out", which reads like a bad URL. Bring
# the compose one up rather than guessing at a URL.
echo "→ ensuring the compose Postgres is up (:5432, what .env names)"
docker compose -f docker-compose.yml up -d postgres >/dev/null 2>&1 || true
for _ in $(seq 1 30); do
  docker compose -f docker-compose.yml exec -T postgres pg_isready -U admin -d default >/dev/null 2>&1 && break
  sleep 2
done
docker compose -f docker-compose.yml exec -T postgres pg_isready -U admin -d default >/dev/null 2>&1 \
  || { echo "✗ postgres did not become ready"; exit 1; }
DB_URL=$(grep -E '^OXY_DATABASE_URL=' .env 2>/dev/null | head -1 | cut -d= -f2- | tr -d '"'"'"'' || true)

echo "→ booting ClickHouse (compose, native TCP on :9010)"
docker compose -f docker-compose.clickhouse.yml up -d >/dev/null
# A cold pull + first boot of this image takes well past 90s on a laptop; the
# container reports healthy long before `exec` will attach. Wait generously —
# giving up early looks exactly like the auth failure above.
for _ in $(seq 1 120); do ch --query "SELECT 1" >/dev/null 2>&1 && break; sleep 3; done
ch --query "SELECT 1" >/dev/null 2>&1 || { echo "✗ ClickHouse did not come up"; exit 1; }

echo "→ starting oxy serve wired to it (boot creates the obs schema)"
pkill -f "oxy serve --enterprise" 2>/dev/null || true
sleep 2
# The compose container runs the stock `default` user with NO password, but
# `.env` names a different ClickHouse with real credentials — and the binary
# loads `.env` itself. `dotenv()` does not override an already-set variable, so
# every one of these is set explicitly here rather than left to be inherited;
# leaving the password merely unset lets `.env`'s win and the boot dies on
# "Authentication failed", which reads like the container is broken.
( set -a; . ./.env.clickhouse; set +a
  export OXY_OBSERVABILITY_BACKEND=clickhouse
  export OXY_CLICKHOUSE_URL="http://localhost:8123"
  export OXY_CLICKHOUSE_USER=default
  export OXY_CLICKHOUSE_PASSWORD="${OXY_CLICKHOUSE_PASSWORD:-oxy-local}"
  export OXY_CLICKHOUSE_DATABASE=observability
  [ -n "$DB_URL" ] && export OXY_DATABASE_URL="$DB_URL"
  # TWO addresses. OXY_OWNER is staff, and the SPA bounces staff off every
  # tenant workspace URL into /admin — so a session minted as the owner cannot
  # reach `/ide/observability/*` at all. `flow@oxy.local` is the org owner
  # `scripts/seed-fixtures.sh` binds to the Local org with no platform
  # standing, and is what the observability flows sign in as.
  OXY_DEV_LOGIN_EMAILS="${OWNER:-hello@oxy.tech},flow@oxy.local" \
    ./target/debug/oxy serve --enterprise > "${TMPDIR:-/tmp}/oxy-obs-backend.log" 2>&1 ) &
for _ in $(seq 1 90); do
  curl -sf --max-time 3 http://localhost:3000/api/health >/dev/null 2>&1 && break; sleep 4
done
curl -sf --max-time 3 http://localhost:3000/api/health >/dev/null 2>&1 \
  || { echo "✗ server did not come up — see ${TMPDIR:-/tmp}/oxy-obs-backend.log"; exit 1; }

for _ in $(seq 1 30); do
  ch --query "EXISTS TABLE observability.observability_spans" 2>/dev/null | grep -q 1 && break; sleep 2
done
ch --query "EXISTS TABLE observability.observability_spans" 2>/dev/null | grep -q 1 \
  || { echo "✗ obs schema absent after boot — is OXY_OBSERVABILITY_BACKEND=clickhouse?"; exit 1; }

# ── Idempotency ─────────────────────────────────────────────────────────────
# INSERT has no upsert here (MergeTree, no dedup key), so running this twice
# would double every trace. Rather than delete anything, the whole insert block
# is skipped when the expected shape is ALREADY present — the three numbers the
# verification at the bottom checks. A ClickHouse carrying a previous run's
# spans in a DIFFERENT shape (fewer distinct prompts, no metric usage) fails
# this test and is re-seeded, which is the behaviour you want after this file
# changes; clear it with `just clickhouse-down -v` first if you want exactly the
# rows below and nothing else.
already() {
  local sp pr me
  sp=$(ch --query "SELECT count() FROM observability.observability_spans WHERE trace_id LIKE 'seed-%'" 2>/dev/null || echo 0)
  pr=$(ch --query "SELECT uniqExact(JSONExtractString(span_attributes, 'question')) FROM observability.observability_spans WHERE trace_id LIKE 'seed-%' AND JSONExtractString(span_attributes, 'question') != ''" 2>/dev/null || echo 0)
  me=$(ch --query "SELECT uniqExact(metric_name) FROM observability.observability_metric_usage WHERE trace_id = 'seed-long'" 2>/dev/null || echo 0)
  [ "${sp:-0}" -ge 14 ] && [ "${pr:-0}" -ge 2 ] && [ "${me:-0}" -ge 2 ]
}
if already; then
  echo "→ spans and metric usage already seeded — skipping the inserts"
  SEED_INSERTS=0
else
  SEED_INSERTS=1
fi

[ "$SEED_INSERTS" = 1 ] && echo "→ inserting spans (2 traces: one 2-span, one 12-span)"
ins() { # trace, span, name, attrs, events, duration_ns, status
  [ "$SEED_INSERTS" = 1 ] || return 0
  ch --query "INSERT INTO observability.observability_spans
    (trace_id,span_id,span_name,span_attributes,event_data,duration_ns,status_code,timestamp)
    VALUES ('$1','$2','$3','$4','$5',$6,'$7',now64(9))"
}
LLM='{"oxy.span_type":"llm","gen_ai.request.model":"claude-sonnet-5"}'
USAGE='[{"name":"llm.usage","attributes":{"prompt_tokens":"1200","completion_tokens":"340"}}]'
OUT='[{"name":"tool_call.output","attributes":{"status":"success","output":"5 rows"}}]'

# THE PROMPTS ARE NOT DECORATION. The traces LIST renders `agent.prompt`, and
# `observability-traces` case 1 waits for the literal text
# "Run a SQL query against the oxymart view". Both traces used to carry the same
# short placeholder, so the list rendered two identical rows, neither matching —
# the flow timed out for 45s on a page that was otherwise perfectly healthy.
# These two strings are the questions the flow's header records as the
# provenance of the traces it was authored against, copied verbatim.
LONG_Q='Run a SQL query against the oxymart view to compute total sales by store, and show me the top 5 stores.'
SHORT_Q='What tables are available? Answer briefly.'
# BOTH `question` AND `agent.prompt`, because the two surfaces read different
# keys and only one of them is obvious. The traces LIST title for an
# `analytics.run` root span comes from `attrs.question` —
# `deriveTraceRow` (traces/components/traceRow.ts:34): `isAnalytics ?
# attrs.question : getPrompt(trace)`. Seeding only `agent.prompt` produced a
# list where every row fell back to the span label ("Analytics"), the API
# response carried the right text all along, and the flow timed out for 45s
# waiting for a question the page was never going to print.
tool_attrs() { # question
  printf '{"oxy.span_type":"tool_call","oxy.execution_type":"semantic_query","oxy.is_verified":"true","oxy.agent.ref":"analytics","question":"%s","agent.prompt":"%s","oxy.database":"local"}' "$1" "$1"
}
TOOL_LONG=$(tool_attrs "$LONG_Q")
TOOL_SHORT=$(tool_attrs "$SHORT_Q")

# A short trace, so the list has more than one row.
ins seed-short seed-short-run  analytics.run       "$TOOL_SHORT" "$OUT"   2100000000 OK
ins seed-short seed-short-llm  llm.call            "$LLM"  "$USAGE"  640000000 OK

# A long one, so the waterfall has real depth to render (the regression these
# flows exist for was a 12-span trace collapsing into a sliver).
ins seed-long seed-long-run      analytics.run       "$TOOL_LONG" "$OUT"  9400000000 OK
ins seed-long seed-long-clarify  analytics.clarify   "$TOOL_LONG" "$OUT"   310000000 OK
for i in 1 2 3 4 5; do
  ins seed-long "seed-long-llm-$i" llm.call "$LLM" "$USAGE" $((400000000 + i * 130000000)) OK
done
ins seed-long seed-long-tool-a   analytics.tool      "$TOOL_LONG" "$OUT"  1250000000 OK
ins seed-long seed-long-tool-b   analytics.tool      "$TOOL_LONG" "$OUT"   870000000 OK
ins seed-long seed-long-execute  analytics.execute   "$TOOL_LONG" "$OUT"  2600000000 OK
ins seed-long seed-long-call     analytics.tool_call "$TOOL_LONG" "$OUT"   980000000 OK
ins seed-long seed-long-interp   analytics.interpret "$TOOL_LONG" "$OUT"   540000000 OK

# ── Metric usage ────────────────────────────────────────────────────────────
# A SECOND table, and nothing wrote to it. `/ide/observability/metrics` reads
# `observability_metric_usage`, not spans — inserting traces does not populate
# it, so `GET /{ws}/metrics/list` answered `{"metrics":[],"total":0}` and both
# `observability-metrics` cases timed out waiting for a metric name.
#
# The two names are the ones the flow's header names as what its authoring runs
# produced: `oxymart.total_sales` and `oxymart.store_id`, both referenced
# together by the "top 5 stores by total sales" question — which is exactly the
# long trace above, so they are tied to its trace_id rather than floating free.
# Uneven counts on purpose: `get_metrics_analytics` picks a single
# most-popular metric with `ORDER BY cnt DESC LIMIT 1`, and a tie makes which
# one wins arbitrary between runs.
[ "$SEED_INSERTS" = 1 ] && echo "→ inserting metric usage (2 metrics, tied to the long trace)"
metric() { # metric_name, times
  local i
  [ "$SEED_INSERTS" = 1 ] || return 0
  for ((i = 0; i < $2; i++)); do
    ch --query "INSERT INTO observability.observability_metric_usage
      (metric_name,source_type,source_ref,context,context_types,trace_id,created_at)
      VALUES ('$1','semantic_query','sales_semantics/views/oxymart.view.yml','$LONG_Q','[\"measure\"]','seed-long',now64(3))"
  done
}
metric oxymart.total_sales 3
metric oxymart.store_id 2

SPANS=$(ch --query "SELECT count() FROM observability.observability_spans WHERE trace_id LIKE 'seed-%'")
ROLLUP=$(ch --query "SELECT count() FROM observability.observability_executions WHERE trace_id LIKE 'seed-%'")
METRICS=$(ch --query "SELECT uniqExact(metric_name) FROM observability.observability_metric_usage WHERE trace_id = 'seed-long'")
PROMPTS=$(ch --query "SELECT uniqExact(JSONExtractString(span_attributes, 'question')) FROM observability.observability_spans WHERE trace_id LIKE 'seed-%' AND JSONExtractString(span_attributes, 'question') != ''")
echo "→ $SPANS spans, $ROLLUP rollup rows, $METRICS metrics, $PROMPTS distinct questions"
[ "$SPANS" -ge 14 ] || { echo "✗ expected at least 14 spans, got $SPANS"; exit 1; }
# Two DISTINCT questions, because the traces list renders `question` and a
# flow that waits for one specific question cannot tell two identical rows
# apart. One distinct value here means the list is right but unmatchable.
[ "$PROMPTS" -ge 2 ] || { echo "✗ the two traces carry the same question — the traces list would show two identical rows"; exit 1; }
[ "$METRICS" -ge 2 ] || { echo "✗ metric usage empty — /ide/observability/metrics would render an empty list"; exit 1; }
# The rollup MV is what the execution-analytics page reads. Zero here means the
# spans landed but nothing will render, which is worth failing on now rather
# than as a locator timeout three minutes later.
[ "$ROLLUP" -ge 1 ] || { echo "✗ rollup MV produced no rows — the analytics page would render empty"; exit 1; }
echo "✓ observability seeded"
