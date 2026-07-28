#!/usr/bin/env bash
# End-to-end verification of the opportunity/drill API against the example_new
# fixture. Asserts on the engine's real JSON — it does NOT re-derive the answer
# in DuckDB and call that end-to-end.
#
# Prerequisites (this script does NOT start anything):
#   1. A running server:  cd example_new && OXY_DATABASE_URL=... ../target/debug/oxy serve --local
#   2. DuckDB's `icu` extension present in ~/.duckdb/extensions/<ver>/<platform>/.
#      In a network-restricted sandbox, extensions.duckdb.org is blocked; the
#      extension can be sideloaded from the PyPI wheel `duckdb-extension-icu`
#      pinned to the same DuckDB version the oxy binary links.
#
# NOTE ON PORTS: the task brief said the unauthenticated internal API is on
# 3001. Measured behaviour differs — on this build, port 3001 does not serve the
# workspace route tree (even /live 404s), while port 3000 serves
# /api/{nil-uuid}/... unauthenticated in --local mode. Override with OXY_URL.

set -uo pipefail

OXY_URL="${OXY_URL:-http://127.0.0.1:3000}"
WS="${OXY_WORKSPACE_ID:-00000000-0000-0000-0000-000000000000}"
BASE="$OXY_URL/api/$WS/semantic/metric-tree"
PERIOD='["2025-07-20","2026-07-19"]'   # 365 days; data spans 400 days ending 2026-07-19
TD='checks.check_date'
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

FAILURES=0
fail() { echo "FAIL: $*" >&2; FAILURES=$((FAILURES + 1)); }
pass() { echo "ok:   $*"; }

command -v jq >/dev/null || { echo "FAIL: jq is required" >&2; exit 2; }

post() { # post <endpoint> <body> <outfile>
  local code
  code=$(curl -s -X POST "$BASE/$1" -H 'Content-Type: application/json' \
    -d "$2" -o "$3" -w '%{http_code}')
  if [ "$code" != "200" ]; then
    echo "FAIL: POST $1 returned HTTP $code: $(head -c 400 "$3")" >&2
    if [ "$code" = "500" ]; then
      echo "HINT: a 500 here is often DuckDB's 'icu' extension missing, not a semantic-layer bug \
— see this script's header comment (prerequisite 2) for the sideload remedy." >&2
    fi
    exit 3
  fi
}

echo "== opportunity scan: checks.net_revenue =="
post opportunity \
  "{\"target\":\"checks.net_revenue\",\"time_dimension\":\"$TD\",\"period\":$PERIOD}" \
  "$TMP/opp.json"

# --- 1. rate mode engaged --------------------------------------------------
WB=$(jq -r '.weight_basis' "$TMP/opp.json")
RATE_DENOM=$(jq -r '.rate_denominator // "none"' "$TMP/opp.json")
if [ "$WB" = "rows" ]; then
  pass "weight_basis=rows (rate_denominator=$RATE_DENOM)"
else
  fail "weight_basis is '$WB', expected 'rows'. Rate mode did not engage — check that \
checks.total_checks is still the FIRST declared count measure on the checks view."
fi
# weight_basis alone can't distinguish a real rate scan from the refusal path
# (the engine returns weight_basis "rows" even with no count measure), so the
# denominator itself must be pinned.
if [ "$RATE_DENOM" = "checks.total_checks" ]; then
  pass "rate_denominator=checks.total_checks"
else
  fail "rate_denominator is '$RATE_DENOM', expected 'checks.total_checks' — either the count \
measure moved/renamed on the checks view, or this is the no-count-measure refusal path \
masquerading as a real rate scan."
fi

# --- 2. five dimensions, including region, order_channel, tenure_band ------
NDIM=$(jq '.dimensions | length' "$TMP/opp.json")
[ "$NDIM" -eq 5 ] || fail "expected 5 dimensions, got $NDIM"

# The engine qualifies dimensions by view. region/city/location_name are declared
# on BOTH checks and locations, so match on the bare name.
have_dim() { jq -e --arg d "$1" '[.dimensions[].dimension | split(".")[-1]] | index($d)' "$TMP/opp.json" >/dev/null; }
for d in region order_channel tenure_band; do
  if have_dim "$d"; then pass "dimension '$d' present"
  else fail "dimension '$d' absent from the top-5. Returned: $(jq -rc '[.dimensions[].dimension]' "$TMP/opp.json")"
  fi
done

# Duplicate-declaration guard. checks.<d> and locations.<d> are the same column
# reached two ways; each duplicate pair burns a top-K slot and pushes a real
# dimension off the end of the list.
NDISTINCT=$(jq '[.dimensions[].dimension | split(".")[-1]] | unique | length' "$TMP/opp.json")
if [ "$NDISTINCT" -lt "$NDIM" ]; then
  fail "only $NDISTINCT distinct dimensions among $NDIM returned — duplicates: \
$(jq -rc '[.dimensions[].dimension|split(".")[-1]]|group_by(.)|map(select(length>1)|.[0])' "$TMP/opp.json"). \
Each duplicate pair costs a top-K slot."
else
  pass "$NDIM dimensions, all distinct"
fi

# --- 3. benchmark basis switches on segment count --------------------------
# The engine's rule is p75 at >=8 segments, best_peer below that. The brief
# named location_name (24 -> p75) and daypart (3 -> best_peer), but daypart
# ranks 6th by upside and TOP_K_DIMENSIONS is 5, so it is legitimately truncated
# out of the response. Asserting the RULE over whatever comes back tests the
# same thing without depending on a dimension surviving the top-K cut.
if [ "$NDIM" -eq 0 ]; then
  fail "benchmark_basis rule can't be checked — .dimensions is empty, so the check \
would pass vacuously."
else
  BAD_BASIS=$(jq -rc '[.dimensions[]
    | select((.cardinality >= 8 and .benchmark_basis != "p75")
          or (.cardinality <  8 and .benchmark_basis != "best_peer"))
    | {dimension, cardinality, benchmark_basis}]' "$TMP/opp.json")
  if [ "$BAD_BASIS" = "[]" ]; then
    pass "benchmark_basis follows the >=8-segment p75 rule: $(jq -rc '[.dimensions[]|"\(.dimension|split(".")[-1])=\(.benchmark_basis)(\(.cardinality))"]' "$TMP/opp.json")"
  else
    fail "benchmark_basis violates the >=8-segment p75 rule for: $BAD_BASIS"
  fi
fi
# location_name (24 segments) is the brief's named p75 case and should be present.
check_basis() { # check_basis <bare-dim> <expected>
  local got
  got=$(jq -r --arg d "$1" '[.dimensions[] | select((.dimension|split(".")[-1])==$d)][0].benchmark_basis // "absent"' "$TMP/opp.json")
  if [ "$got" = "$2" ]; then pass "benchmark_basis($1)=$2"
  else fail "benchmark_basis($1) is '$got', expected '$2'"
  fi
}
check_basis location_name p75

# --- 4. the significance gate is live --------------------------------------
# A zero here is a SILENT failure: an inert gate looks like a clean result.
DROPPED=$(jq '[.dimensions[].segments_dropped_as_noise] | add // 0' "$TMP/opp.json")
if [ "$DROPPED" -gt 0 ]; then pass "segments_dropped_as_noise total = $DROPPED (gate is live)"
else fail "segments_dropped_as_noise is 0 everywhere — the significance gate is inert."
fi

# --- 4b. the deliberate nulls are rejected ---------------------------------
if jq -e '[.skipped_dimensions[]?.dimension | split(".")[-1]] | index("table_section")' "$TMP/opp.json" >/dev/null; then
  pass "table_section rejected: $(jq -r '[.skipped_dimensions[]|select((.dimension|split(".")[-1])=="table_section")][0].reason' "$TMP/opp.json")"
else
  fail "table_section was NOT skipped — the planted no-effect null was not rejected."
fi

# servers.server_name (263 distinct values) is the high-cardinality identifier
# expected to be skipped. Pinned to that dimension, not just any "cardinality"
# reason, so the assertion can't be satisfied by an unrelated skip.
if jq -e '[.skipped_dimensions[]? | select((.dimension|split(".")[-1])=="server_name") | .reason
    | select(test("cardinality"))] | length > 0' "$TMP/opp.json" >/dev/null; then
  pass "servers.server_name skipped as expected: $(jq -r '[.skipped_dimensions[]|select((.dimension|split(".")[-1])=="server_name")][0].reason' "$TMP/opp.json")"
else
  fail "server_name was NOT skipped for cardinality — the >=25-distinct-value guard did not fire. \
Skipped dimensions: $(jq -rc '[.skipped_dimensions[]?.dimension]' "$TMP/opp.json")"
fi

# --- 5. top upside is the right order of magnitude, on the right dimension -
# TOP and TOPDIM must come from the same object — pulling TOP via `max` and
# TOPDIM via `.dimensions[0]` let them disagree whenever the response isn't
# sorted by total_upside, so a regression that put e.g. `checks.city` first
# could pass silently. `max_by` picks one object; both fields read off it.
TOP_DIM_OBJ=$(jq -c '[.dimensions[]] | max_by(.total_upside)' "$TMP/opp.json")
TOP=$(echo "$TOP_DIM_OBJ" | jq '.total_upside')
TOPDIM=$(echo "$TOP_DIM_OBJ" | jq -r '.dimension')
EXPECTED_TOPDIM='checks.order_channel'
if [ "$TOPDIM" != "$EXPECTED_TOPDIM" ]; then
  fail "top total_upside dimension is '$TOPDIM', expected '$EXPECTED_TOPDIM' (delivery's gap x \
volume should own the top spot) — total_upside=$TOP"
elif awk "BEGIN{exit !($TOP >= 300000 && $TOP <= 900000)}"; then
  pass "top total_upside = $TOP on $TOPDIM (measured actual: 392,787 — see task-7-report.md)"
else
  fail "top total_upside $TOP on $TOPDIM outside the 300k-900k band — order-of-magnitude miss."
fi

# --- 6/7. the delivery drill reaches the alcohol branch --------------------
echo
echo "== drill: order_channel=delivery, root checks.net_revenue =="
post drill \
  "{\"target\":\"checks.net_revenue\",\"time_dimension\":\"$TD\",\"period\":$PERIOD,\
\"root\":{\"dimension\":\"checks.order_channel\",\"segment\":\"delivery\"}}" \
  "$TMP/drill.json"

EXPECTED='["checks.net_revenue","checks.addon_revenue","checks.beverages_revenue","checks.alcoholic_revenue"]'
ACTUAL=$(jq -c '[.levels[].measure]' "$TMP/drill.json")
if [ "$ACTUAL" = "$(echo "$EXPECTED" | jq -c .)" ]; then
  pass "drill descended 4 levels: $ACTUAL"
else
  fail "drill levels are $ACTUAL, expected $EXPECTED. If it stops at beverages_revenue, \
the 'type: custom' conversion did not take and there is no Component edge."
fi

# Each level's candidate concentrations must partition that level's gap.
jq -c '.levels[] | {m:.measure, n:(.candidates|length), s:([.candidates[].concentration]|add)}' "$TMP/drill.json" \
| while read -r row; do
  m=$(echo "$row" | jq -r .m); n=$(echo "$row" | jq -r .n); s=$(echo "$row" | jq -r '.s // "null"')
  if [ "$n" -eq 0 ]; then echo "ok:   $m is a leaf (no candidates)"; continue; fi
  if awk "BEGIN{exit !($s > 0.99 && $s < 1.01)}"; then echo "ok:   $m concentrations sum to $s"
  else echo "FAIL: $m concentrations sum to $s, expected ~1.0" >&2; exit 1; fi
done || fail "a drill level's concentrations did not sum to ~1.0"

# --- 8. the revenue/profit inversion survives ------------------------------
echo
echo "== gross_profit: add-on leaf ranking must put beverages ahead of sides =="
post drill \
  "{\"target\":\"checks.gross_profit\",\"time_dimension\":\"$TD\",\"period\":$PERIOD,\
\"root\":{\"dimension\":\"checks.order_channel\",\"segment\":\"delivery\"}}" \
  "$TMP/drill_gp.json"

leaf_conc() { # leaf_conc <file> <parent-measure> <child-measure>
  jq -r --arg p "$2" --arg c "$3" \
    '[.levels[]|select(.measure==$p)][0].candidates[]?|select(.kind.Component.measure==$c)|.concentration' "$1"
}
REV_BEV=$(leaf_conc "$TMP/drill.json"    checks.addon_revenue checks.beverages_revenue)
REV_SID=$(leaf_conc "$TMP/drill.json"    checks.addon_revenue checks.sides_revenue)
GP_BEV=$(leaf_conc  "$TMP/drill_gp.json" checks.addon_profit  checks.beverages_profit)
GP_SID=$(leaf_conc  "$TMP/drill_gp.json" checks.addon_profit  checks.sides_profit)
echo "      revenue: beverages=$REV_BEV sides=$REV_SID"
echo "      profit:  beverages=$GP_BEV sides=$GP_SID"

if [ -z "$REV_BEV" ] || [ -z "$REV_SID" ]; then
  fail "revenue drill did not expose the add-on revenue leaves — got $(jq -c '[.levels[].measure]' "$TMP/drill.json")"
elif [ -z "$GP_BEV" ] || [ -z "$GP_SID" ]; then
  fail "gross_profit drill did not expose the add-on profit leaves — got $(jq -c '[.levels[].measure]' "$TMP/drill_gp.json")"
elif awk "BEGIN{exit !($GP_BEV > $GP_SID)}"; then
  pass "beverages ahead of sides on profit ($GP_BEV > $GP_SID)"
else
  fail "sides outranks beverages on gross_profit — the revenue/profit inversion did not survive."
fi
# The inversion is a WIDENING: beverages must gain share moving revenue -> profit.
if [ -z "$REV_BEV" ] || [ -z "$REV_SID" ] || [ -z "$GP_BEV" ] || [ -z "$GP_SID" ]; then
  fail "can't check the revenue->profit widening — one or more leaf concentrations is missing \
(already reported above)."
elif awk "BEGIN{exit !($GP_BEV > $REV_BEV && $GP_SID < $REV_SID)}"; then
  pass "inversion widens on profit (beverages $REV_BEV->$GP_BEV, sides $REV_SID->$GP_SID)"
else
  fail "beverages/sides shares did not move as designed from revenue to profit."
fi

echo
if [ "$FAILURES" -eq 0 ]; then echo "ALL ASSERTIONS PASSED"; exit 0; fi
echo "$FAILURES ASSERTION(S) FAILED" >&2
exit 1
