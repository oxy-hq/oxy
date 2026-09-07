#!/usr/bin/env bash
# End-to-end verification of the scenario-simulation API (baseline + predict)
# against the store-day fixture. The companion to verify_opportunity_api.sh:
# same posture — it asserts on the engine's real JSON and never re-derives the
# answer itself.
#
# What it is for: propagation branches on the arithmetic operator of each edge
# and on whether an edge is a declared driver, and every branch is a different
# claim to the user ("exact" vs "estimated" vs "can't size this" vs silence).
# `store_days.view.yml` is built so all of them occur; this script is what says
# they still do.
#
# Prerequisites (this script does NOT start anything) — identical to
# verify_opportunity_api.sh, read its header for the DuckDB `icu` sideload note:
#   1. A running server:  cd example_new && OXY_DATABASE_URL=... ../target/debug/oxy serve --local
#   2. DuckDB's `icu` extension present.

set -uo pipefail

OXY_URL="${OXY_URL:-http://127.0.0.1:3000}"
WS="${OXY_WORKSPACE_ID:-00000000-0000-0000-0000-000000000000}"
BASE="$OXY_URL/api/$WS/semantic/metric-tree"
TD='store_days.business_date'
# Data spans 400 days ending 2026-07-19. RECENT is inside the banquet
# wind-down (zero denominator); FULL contains it (sizes normally). The pair is
# the point — one window is not enough to test the zero-child case, because
# "unquantifiable everywhere" and "unquantifiable when it should be" look the
# same from one window.
RECENT='["2026-04-21","2026-07-19"]'
FULL='["2025-07-20","2026-07-19"]'
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
    exit 3
  fi
}

# baseline <roots-json> <period> <outfile>
baseline() {
  post baseline "{\"roots\":$1,\"time_dimension\":\"$TD\",\"period\":$2}" "$3"
}

# predict_with <baseline-file> <changes-json> <outfile>
#
# Splices the baseline's `values` map into the predict body. The two calls are
# separate on purpose — the UI re-runs only `predict` while a lever value is
# being edited — so a verification that skipped the baseline would only ever
# exercise delta-only mode.
predict_with() {
  jq -c --argjson changes "$2" \
    '{changes: $changes, values: .values, coefficients: (.fitted // [])}' \
    "$1" > "$TMP/req.json"
  post predict "@$TMP/req.json" "$3"
}

# fit <baseline-file> <from> <to> <field> — read one fitted-driver field.
#
# Keyed on BOTH endpoints. `.from` alone is not a key: a lever with two
# undeclared outgoing edges has two entries in `.fitted`, `jq -r` prints one
# line each, and `[ "$FORM" = "quadratic" ]` then compares against
# "quadratic\nlinear" and fails while blaming the engine for inferring a
# straight line. Nothing orders `.fitted`, so which line comes first isn't even
# stable.
fit() {
  jq -r --arg f "$2" --arg t "$3" --arg k "$4" \
    '.fitted[]? | select(.from==$f and .to==$t) | .[$k] // empty' "$1"
}

# impact <file> <measure> <field>
impact() { jq -r --arg m "$2" --arg f "$3" '.impacts[] | select(.measure==$m) | .[$f]' "$1"; }

# assert_case <label> <file> <measure> <expected-confidence> <sign: +|-|0>
#
# Confidence and sign are asserted together because either alone passes for the
# wrong reason: a `0` delta is correct for `unquantifiable` and a bug for
# anything else, and an `exact` label on a sized multiplicative edge would
# overstate a first-order approximation.
assert_case() {
  local label=$1 file=$2 measure=$3 want_conf=$4 want_sign=$5
  local conf delta bad=0
  conf=$(impact "$file" "$measure" confidence)
  delta=$(impact "$file" "$measure" estimated_delta)
  if [ -z "$conf" ]; then
    fail "$label: '$measure' is absent from .impacts entirely"
    return
  fi
  if [ "$conf" != "$want_conf" ]; then
    fail "$label: '$measure' confidence is '$conf', expected '$want_conf'"
    bad=1
  fi
  case $want_sign in
    +) awk -v d="$delta" 'BEGIN{exit !(d+0 > 0)}' || { fail "$label: '$measure' delta $delta, expected > 0"; bad=1; } ;;
    -) awk -v d="$delta" 'BEGIN{exit !(d+0 < 0)}' || { fail "$label: '$measure' delta $delta, expected < 0"; bad=1; } ;;
    0) awk -v d="$delta" 'BEGIN{exit !(d+0 == 0)}' || { fail "$label: '$measure' delta $delta, expected exactly 0"; bad=1; } ;;
  esac
  [ "$bad" -eq 0 ] && pass "$label: $measure $delta [$conf]"
}

echo "== baseline: store_days.net_sales over the recent window =="
baseline '["store_days.net_sales"]' "$RECENT" "$TMP/base_sales.json"

NOTE=$(jq -r '.baseline_note // "none"' "$TMP/base_sales.json")
N_VALUED=$(jq -r '.values | length' "$TMP/base_sales.json")
if [ "$NOTE" = "none" ] && [ "$N_VALUED" -gt 0 ]; then
  pass "baseline valued $N_VALUED reachable measures with no note"
else
  fail "baseline returned $N_VALUED values, note: $NOTE — everything below depends on it"
fi

echo
echo "== 1. additive branch: a cost rises, its parent falls, both exact =="
baseline '["store_days.food_cost"]' "$RECENT" "$TMP/base_food.json"
predict_with "$TMP/base_food.json" '[{"measure":"store_days.food_cost","delta":50000}]' "$TMP/p_food.json"
assert_case "add (sign +1)" "$TMP/p_food.json" store_days.prime_cost exact +
assert_case "sub (sign -1)" "$TMP/p_food.json" store_days.store_profit exact -

echo
echo "== 2. multiplicative branch: ratios size only against a baseline =="
assert_case "div (numerator)" "$TMP/p_food.json" store_days.prime_cost_pct estimated +
assert_case "div (denominator)" "$TMP/p_food.json" store_days.contribution_margin_pct estimated -
assert_case "mul (constant factor)" "$TMP/p_food.json" store_days.annualized_store_profit estimated -

# Same lever, no `values` at all — the delta-only mode the UI falls into when
# the layer has no usable time dimension. The additive branch must be
# unchanged; every multiplicative edge must degrade to unquantifiable rather
# than to zero.
post predict '{"changes":[{"measure":"store_days.food_cost","delta":50000}]}' "$TMP/p_food_nv.json"
assert_case "delta-only: additive still exact" "$TMP/p_food_nv.json" store_days.store_profit exact -
assert_case "delta-only: ratio unsizable" "$TMP/p_food_nv.json" store_days.prime_cost_pct unquantifiable 0

echo
echo "== 3. declared drivers: coefficient, and cumulative lag over four hops =="
baseline '["store_days.loyalty_signups"]' "$RECENT" "$TMP/base_loyal.json"
predict_with "$TMP/base_loyal.json" '[{"measure":"store_days.loyalty_signups","delta":1000}]' "$TMP/p_loyal.json"
assert_case "driver hop 1" "$TMP/p_loyal.json" store_days.promo_redemptions estimated +
assert_case "driver hop 3" "$TMP/p_loyal.json" store_days.net_sales estimated +
LAG=$(impact "$TMP/p_loyal.json" store_days.net_sales lag)
if [ "$LAG" = "24" ]; then
  pass "lag accumulates across hops (21d + 3d = ${LAG}d)"
else
  fail "net_sales lag is '$LAG', expected 24 — the 21d and 3d driver edges must sum, \
not report the last edge's"
fi

echo
echo "== 4. a driver the fit REFUSES propagates nothing, and says why =="
baseline '["store_days.weather_severity_index"]' "$RECENT" "$TMP/base_wx.json"
predict_with "$TMP/base_wx.json" '[{"measure":"store_days.weather_severity_index","delta":0.2}]' "$TMP/p_wx.json"

# The refusal must be REPORTED, not merely implied by an empty impact list.
# Silence and a refusal look identical on the canvas; only one of them tells
# the user the model declined rather than the UI failing to render.
WX_REFUSAL=$(fit "$TMP/base_wx.json" store_days.weather_severity_index store_days.guest_count refusal)
WX_COEF=$(fit "$TMP/base_wx.json" store_days.weather_severity_index store_days.guest_count coefficient)
if [ -n "$WX_REFUSAL" ] && [ -z "$WX_COEF" ]; then
  pass "weather_severity_index refused with a reason: $WX_REFUSAL"
else
  fail "weather_severity_index came back with coefficient='$WX_COEF' refusal='$WX_REFUSAL' — \
this series is inert by construction (t~0.5), so the fit must refuse it and name why; fitting \
it would forecast a confident number from noise"
fi

N_WX=$(jq -r '.impacts | length' "$TMP/p_wx.json")
if [ "$N_WX" = "0" ]; then
  pass "weather_severity_index yields no impacts (refused, so no magnitude to propagate)"
else
  fail "weather_severity_index produced $N_WX impacts — a refused fit must propagate nothing"
fi

echo
echo "== 5. zero child on a multiplicative edge: unsizable, then sizable =="
baseline '["store_days.banquet_covers"]' "$RECENT" "$TMP/base_bq_recent.json"
predict_with "$TMP/base_bq_recent.json" '[{"measure":"store_days.banquet_covers","delta":500}]' "$TMP/p_bq_recent.json"
assert_case "wind-down window" "$TMP/p_bq_recent.json" store_days.banquet_check_average unquantifiable 0

baseline '["store_days.banquet_covers"]' "$FULL" "$TMP/base_bq_full.json"
predict_with "$TMP/base_bq_full.json" '[{"measure":"store_days.banquet_covers","delta":500}]' "$TMP/p_bq_full.json"
assert_case "full window" "$TMP/p_bq_full.json" store_days.banquet_check_average estimated -

echo
echo "== 6. two paths into one node are summed, not deduplicated =="
# contribution_margin_pct = store_profit / net_sales, and net_sales reaches it
# BOTH through store_profit (+) and as the denominator (-). The engine must
# report the sum; either path alone is a different number.
baseline '["store_days.net_sales"]' "$RECENT" "$TMP/base_ns.json"
predict_with "$TMP/base_ns.json" '[{"measure":"store_days.net_sales","delta":1000000}]' "$TMP/p_ns.json"
S=$(jq -r '.values["store_days.net_sales"]' "$TMP/base_ns.json")
P=$(jq -r '.values["store_days.store_profit"]' "$TMP/base_ns.json")
GOT=$(impact "$TMP/p_ns.json" store_days.contribution_margin_pct estimated_delta)
EXPECT=$(awk -v s="$S" -v p="$P" 'BEGIN{cm=p/s; d=1000000; printf "%.6f", cm*d/p - cm*d/s}')
VIA_A=$(awk -v s="$S" -v p="$P" 'BEGIN{cm=p/s; printf "%.6f", cm*1000000/p}')
if awk -v g="$GOT" -v e="$EXPECT" 'BEGIN{exit !(g-e < 1e-4 && e-g < 1e-4)}'; then
  pass "contribution_margin_pct $GOT == both paths summed ($EXPECT); one path alone is $VIA_A"
else
  fail "contribution_margin_pct is $GOT, expected the two-path sum $EXPECT (one path alone \
would be $VIA_A) — a traversal that visits each node once under-counts this"
fi

echo
echo "== 7. a coefficient nobody declared, measured from history =="
# store_days.marketing_spend -> net_sales declares `lag: 7` and NO
# coefficient. Before runtime fitting this edge propagated nothing at all;
# the engine now measures it over the baseline window.
baseline '["store_days.marketing_spend"]' "$FULL" "$TMP/base_mkt.json"
MKT_COEF=$(fit "$TMP/base_mkt.json" store_days.marketing_spend store_days.net_sales coefficient)
MKT_T=$(fit "$TMP/base_mkt.json" store_days.marketing_spend store_days.net_sales t_stat)
MKT_N=$(fit "$TMP/base_mkt.json" store_days.marketing_spend store_days.net_sales n)

if [ -z "$MKT_COEF" ]; then
  fail "marketing_spend -> net_sales was not fitted (t='$MKT_T'). The edge declares no \
coefficient, so without a fit it propagates nothing and this whole case is untested."
else
  # gen_restaurant_data.py measures this series at 5.783 by an independent
  # Python within-location OLS. Agreement is the real assertion: it says the
  # engine's panel fit and the generator's construction describe the same
  # data. The band is wide enough to absorb the engine regressing against raw
  # daily sales where the generator uses a 7-day mean.
  if awk -v c="$MKT_COEF" 'BEGIN{exit !(c > 5.2 && c < 6.4)}'; then
    pass "marketing_spend -> net_sales fitted at $MKT_COEF (t $MKT_T, n $MKT_N), \
matching the generator's independent 5.783"
  else
    fail "marketing_spend -> net_sales fitted at $MKT_COEF, outside 5.2..6.4 — \
gen_restaurant_data.py measures 5.783 for this series by an independent OLS, so the engine \
and the data disagree and every forecast across this edge is wrong by that ratio"
  fi
fi

# A fitted coefficient must behave exactly like a declared one downstream, or
# the forecast depends on where the number came from.
predict_with "$TMP/base_mkt.json" '[{"measure":"store_days.marketing_spend","delta":100000}]' "$TMP/p_mkt.json"
assert_case "fitted edge propagates" "$TMP/p_mkt.json" store_days.net_sales estimated +
assert_case "and keeps going downstream" "$TMP/p_mkt.json" store_days.store_profit estimated +
MKT_LAG=$(impact "$TMP/p_mkt.json" store_days.net_sales lag)
if [ "$MKT_LAG" = "7" ]; then
  pass "the declared lag survives fitting (${MKT_LAG}d)"
else
  fail "net_sales lag is '$MKT_LAG', expected 7 — \`lag:\` is declared on this edge even though \
the coefficient is not, and the fit must pair rows at it rather than discard it"
fi

echo
echo "== 8. a saturating edge, fitted and applied as an elasticity =="
# `delivery_app_spend -> delivery_orders` declares no `form:` and no
# coefficient, so the engine infers the log-log shape as well as the size. Everything else in this fixture is linear, where fitting in logs
# and fitting in levels agree — so this is the only case that can tell an engine
# that honours `form:` from one that ignores it.
baseline '["store_days.delivery_app_spend"]' "$FULL" "$TMP/base_dlv.json"
DLV_COEF=$(fit "$TMP/base_dlv.json" store_days.delivery_app_spend store_days.delivery_orders coefficient)
DLV_FORM=$(fit "$TMP/base_dlv.json" store_days.delivery_app_spend store_days.delivery_orders form)
DLV_DROPPED=$(fit "$TMP/base_dlv.json" store_days.delivery_app_spend store_days.delivery_orders n_nonpositive)

# The generator constructs the curve at 0.45 and measures 0.451 by its own
# independent Python OLS in logs. A form-blind fit measures 0.109 on the same
# rows, so the window below excludes it by a wide margin rather than by luck.
if [ -z "$DLV_COEF" ]; then
  fail "delivery_app_spend -> delivery_orders was not fitted at all — this is the fixture's \
only non-linear edge, so the whole log-log path is untested without it"
elif awk -v c="$DLV_COEF" 'BEGIN{exit !(c > 0.40 && c < 0.50)}'; then
  pass "fitted as an elasticity: $DLV_COEF (constructed 0.45)"
else
  fail "delivery_app_spend -> delivery_orders fitted at $DLV_COEF, outside 0.40..0.50. A level \
slope on these rows is ~0.109 — if that is what came back, the fit measured raw levels \
rather than the log-log shape it should have inferred"
fi

if [ "$DLV_FORM" = "log-log" ]; then
  pass "the fit reports the form it was measured in ($DLV_FORM)"
else
  fail "fitted form is '$DLV_FORM', expected 'log-log' — the coefficient is meaningless without \
it, and apply_fitted_coefficients drops a fit whose form does not match the edge"
fi

# ~4% of store-days are dark (spend 0). `ln(0)` is undefined, so those pairs
# must leave the fit AND be counted: `n` is what the observation gate reads, so
# a silent drop moves the gate for a reason nothing reports.
if [ -n "$DLV_DROPPED" ] && [ "$DLV_DROPPED" -gt 0 ]; then
  pass "the log fit dropped and reported $DLV_DROPPED non-positive pair(s)"
else
  fail "n_nonpositive is '$DLV_DROPPED' — the fixture plants ~4% dark days (spend 0) precisely \
so this is non-zero; a 0 means the rows were dropped silently or not at all"
fi

# Applied, the elasticity must be read as a proportion. +10% on spend should
# move orders by (1.10^0.45 - 1) = ~4.4% of the current order count — NOT by
# 0.45 orders per dollar, which is what a level reading gives.
#
# The expectation below is the EXACT log-link form `y * ((1+r)^beta - 1)`, which
# is what the engine computes for a `[ln x]` basis. The first-order `y * beta * r`
# it replaced is a different number — 0.9739x of it here — so a tolerance wide
# enough to accept both would no longer distinguish either from a bug. The band
# stays tight because the exact value is deterministic, not because the gap to
# first-order is small: at +10% it is 2.6%, and it grows with r.
DLV_SPEND=$(jq -r '.values["store_days.delivery_app_spend"]' "$TMP/base_dlv.json")
DLV_ORDERS=$(jq -r '.values["store_days.delivery_orders"]' "$TMP/base_dlv.json")
DLV_DELTA=$(awk -v s="$DLV_SPEND" 'BEGIN{printf "%.4f", s * 0.10}')
predict_with "$TMP/base_dlv.json" \
  "[{\"measure\":\"store_days.delivery_app_spend\",\"delta\":$DLV_DELTA}]" "$TMP/p_dlv.json"
assert_case "elasticity propagates" "$TMP/p_dlv.json" store_days.delivery_orders estimated +

GOT=$(impact "$TMP/p_dlv.json" store_days.delivery_orders estimated_delta)
WANT=$(awk -v o="$DLV_ORDERS" -v c="$DLV_COEF" -v s="$DLV_SPEND" -v d="$DLV_DELTA" \
  'BEGIN{printf "%.4f", o * ((1 + d/s) ^ c - 1)}')
if awk -v g="$GOT" -v w="$WANT" 'BEGIN{exit !(w != 0 && (g/w) > 0.99 && (g/w) < 1.01)}'; then
  pass "+10% spend moves orders by $GOT, matching orders x ((1+10%)^elasticity - 1) ($WANT)"
else
  fail "+10% spend moved orders by $GOT, expected ~$WANT (= $DLV_ORDERS x (1.10^$DLV_COEF - 1)). A \
log-log coefficient is a proportion; applying it as a level slope would give \
$(awk -v c="$DLV_COEF" -v d="$DLV_DELTA" 'BEGIN{printf "%.2f", c*d}'), and reading it \
first-order rather than exactly would give \
$(awk -v o="$DLV_ORDERS" -v c="$DLV_COEF" 'BEGIN{printf "%.4f", o * c * 0.10}')"
fi

# A claim about proportions needs the levels it is proportional to. With no
# `values` this edge must say "can't size", where the LINEAR marketing edge
# above keeps answering — the same split the component operators show.
jq -c '{changes: [{measure: "store_days.delivery_app_spend", delta: 100}],
        coefficients: (.fitted // [])}' "$TMP/base_dlv.json" > "$TMP/req_dlv_nv.json"
post predict "@$TMP/req_dlv_nv.json" "$TMP/p_dlv_nv.json"
NV_CONF=$(impact "$TMP/p_dlv_nv.json" store_days.delivery_orders confidence)
NV_DELTA=$(impact "$TMP/p_dlv_nv.json" store_days.delivery_orders estimated_delta)
if [ "$NV_CONF" = "unquantifiable" ]; then
  pass "with no values, the elasticity reports unquantifiable rather than a level number"
else
  fail "delta-only mode reported confidence '$NV_CONF' delta '$NV_DELTA' for a log-log edge. \
An elasticity cannot be sized without the levels it scales; anything but 'unquantifiable' here \
means it was quietly evaluated as if it were linear"
fi

echo
echo "== 9. a lever that stops helping: two coefficients and a ceiling =="
# `discount_depth -> promo_margin` declares NOTHING — no coefficients and no
# `form:` — so the engine measures the shape as well as the magnitude. It is the
# only edge in this project that can REVERSE, and the only one needing two
# coefficients; everything above points one way for ever.
baseline '["store_days.discount_depth"]' "$FULL" "$TMP/base_disc.json"
N_COEF=$(jq -r '.fitted[]? | select(.from=="store_days.discount_depth" and .to=="store_days.promo_margin") | .coefficients | length' "$TMP/base_disc.json")
CURVE=$(jq -r '.fitted[]? | select(.from=="store_days.discount_depth" and .to=="store_days.promo_margin") | .coefficients[1] // empty' "$TMP/base_disc.json")
CURVE_T=$(jq -r '.fitted[]? | select(.from=="store_days.discount_depth" and .to=="store_days.promo_margin") | .t_stats[1] // empty' "$TMP/base_disc.json")
S2=$(jq -r '.fitted[]? | select(.from=="store_days.discount_depth" and .to=="store_days.promo_margin") | .moments.s2 // empty' "$TMP/base_disc.json")

FORM=$(jq -r '.fitted[]? | select(.from=="store_days.discount_depth" and .to=="store_days.promo_margin") | .form // empty' "$TMP/base_disc.json")
SRC=$(jq -r '.fitted[]? | select(.from=="store_days.discount_depth" and .to=="store_days.promo_margin") | .form_source // empty' "$TMP/base_disc.json")
NCAND=$(jq -r '.fitted[]? | select(.from=="store_days.discount_depth" and .to=="store_days.promo_margin") | .candidates | length' "$TMP/base_disc.json")
if [ "$FORM" = "quadratic" ] && [ "$SRC" = "inferred" ]; then
  pass "the shape was FOUND, not declared ($FORM, $SRC, $NCAND candidates compared)"
else
  fail "form is '$FORM' from '$SRC'. This edge declares no \`form:\` on purpose — the \
engine is supposed to infer a turning point from the data, and if it settles on a straight \
line instead the whole ceiling story silently disappears"
fi

if [ "$N_COEF" = "2" ]; then
  pass "fitted TWO coefficients (slope and curvature), not one"
else
  fail "the quadratic edge came back with $N_COEF coefficient(s). One number can only \
describe a shape that points one way for ever, so a turning point cannot be expressed — this \
is the case the coefficient vector exists for"
fi

# The curvature IS the turning point. If it is not significant, the engine must
# refuse rather than report a peak assembled from noise.
if [ -n "$CURVE" ] && awk -v c="$CURVE" 'BEGIN{exit !(c < 0)}'; then
  pass "the curvature is negative ($CURVE), so the curve bends back down"
else
  fail "curvature is '$CURVE' — the fixture builds an inverted U, so a non-negative \
curvature means the two-term fit is not recovering the shape"
fi
if [ -n "$CURVE_T" ] && awk -v t="$CURVE_T" 'BEGIN{exit !(t < -8)}'; then
  pass "and it is decisively significant (t $CURVE_T), so the peak is not noise"
else
  fail "curvature t is '$CURVE_T'. Every basis term must clear |t| >= 2 or the engine \
refuses the edge; at this t the turning-point case is not being exercised"
fi

# The moments are what let a per-store-day curvature be applied to a year-wide
# aggregate at all. Without them the same arithmetic is 42,905x out on this
# fixture, with the sign flipped.
if [ -n "$S2" ] && awk -v s="$S2" 'BEGIN{exit !(s > 0)}'; then
  pass "the basis moments survived the round trip (sum of squares $S2)"
else
  fail "moments.s2 is '$S2' — a curved response cannot cross the row-to-aggregate gap \
without it, so predict would either refuse or (worse) substitute the square of the sum"
fi

# The behaviour all of this is for: helps, helps less, then hurts. Deltas are on
# the AVERAGE depth (~23.1), so +8.4 is about +36% and +20 about +87%.
predict_with "$TMP/base_disc.json" '[{"measure":"store_days.discount_depth","delta":1}]' "$TMP/p_d1.json"
predict_with "$TMP/base_disc.json" '[{"measure":"store_days.discount_depth","delta":8.4}]' "$TMP/p_d2.json"
predict_with "$TMP/base_disc.json" '[{"measure":"store_days.discount_depth","delta":12}]' "$TMP/p_d3.json"
predict_with "$TMP/base_disc.json" '[{"measure":"store_days.discount_depth","delta":20}]' "$TMP/p_d4.json"
D1=$(impact "$TMP/p_d1.json" store_days.promo_margin estimated_delta)
D2=$(impact "$TMP/p_d2.json" store_days.promo_margin estimated_delta)
D3=$(impact "$TMP/p_d3.json" store_days.promo_margin estimated_delta)
D4=$(impact "$TMP/p_d4.json" store_days.promo_margin estimated_delta)
printf "      +1 -> %s | +8.4 -> %s | +12 -> %s | +20 -> %s\n" "$D1" "$D2" "$D3" "$D4"

if awk -v a="$D1" -v b="$D2" 'BEGIN{exit !(a > 0 && b > a)}'; then
  pass "a modest deepening helps, and more helps more ($D1 -> $D2)"
else
  fail "the lever does not climb first: +1 gave $D1 and +8.4 gave $D2"
fi
if awk -v b="$D2" -v c="$D3" 'BEGIN{exit !(c > 0 && c < b)}'; then
  pass "past the peak the gain SHRINKS while staying positive ($D3 < $D2)"
else
  fail "no saturation: +8.4 gave $D2 and +12 gave $D3, which should be smaller but positive"
fi
if awk -v d="$D4" 'BEGIN{exit !(d < 0)}'; then
  pass "and pushed far enough the lever HURTS ($D4) — the claim no single coefficient can make"
else
  fail "+20 gave $D4, expected a loss. A shape that cannot go negative is not a turning \
point, and the recommendation would always reward discounting harder"
fi

# A quadratic extrapolated past its own evidence will confidently forecast a
# reversal it never observed, so a lever beyond the fitted spread is refused.
predict_with "$TMP/base_disc.json" '[{"measure":"store_days.discount_depth","delta":200}]' "$TMP/p_d5.json"
OOD=$(impact "$TMP/p_d5.json" store_days.promo_margin confidence)
if [ "$OOD" = "unquantifiable" ]; then
  pass "a lever past the observed spread is refused, not extrapolated"
else
  fail "a +200 move on a depth observed between \$6 and \$39.60 reported '$OOD'. Outside \
its own evidence a quadratic diverges, so this must refuse rather than answer"
fi

# Dropping the coefficients on the way to predict must not silently work.
# This is the wire contract between the two endpoints; if it breaks, the UI
# loses every fitted edge and the canvas goes inert with no error anywhere.
post predict '{"changes":[{"measure":"store_days.marketing_spend","delta":100000}]}' "$TMP/p_mkt_nofit.json"
N_NOFIT=$(jq -r '.impacts | length' "$TMP/p_mkt_nofit.json")
if [ "$N_NOFIT" = "0" ]; then
  pass "without the fitted coefficients, the same lever moves nothing (as it must)"
else
  fail "predict produced $N_NOFIT impacts with no coefficients supplied — the edge declares \
none, so something is inventing a magnitude"
fi

# ── §9 Time-series projection ────────────────────────────────────────────────
#
# The third endpoint. `baseline` answers "what is it worth over the window",
# `predict` "what else moves"; `projection` answers "what has it been doing,
# and what next". It returns the BASELINE curve only — the scenario curve is
# composed on the client from this plus `predict`, so nothing here should ever
# report a lever's effect.
echo
post projection "{\"roots\":[\"store_days.marketing_spend\"],\"time_dimension\":\"$TD\",\
\"period\":$FULL,\"granularity\":\"day\",\"horizon\":30}" "$TMP/proj.json"

NS=$(jq -r '.series[] | select(.measure=="store_days.net_sales")' "$TMP/proj.json")
N_HIST=$(jq -r '.history | length' <<<"$NS")
N_FC=$(jq -r '.forecast | length' <<<"$NS")
if [ "$N_FC" = "30" ]; then
  pass "the horizon is honoured: 30 forecast buckets off $N_HIST days of history"
else
  fail "asked for a 30-bucket horizon, got $N_FC (refusal: $(jq -r '.refusal // "none"' <<<"$NS"))"
fi

# The forecast continues the history's calendar rather than restarting it —
# a projection whose first bucket repeats the last historical date would draw
# two points on one day and shift every subsequent bucket a day early.
LAST_H=$(jq -r '.history[-1].date' <<<"$NS")
FIRST_F=$(jq -r '.forecast[0].date' <<<"$NS")
EXPECTED=$(date -u -j -f '%Y-%m-%d' "$LAST_H" -v+1d '+%Y-%m-%d' 2>/dev/null \
  || date -u -d "$LAST_H + 1 day" '+%Y-%m-%d')
if [ "$FIRST_F" = "$EXPECTED" ]; then
  pass "the forecast starts the day after the history ends ($FIRST_F)"
else
  fail "history ends $LAST_H but the forecast starts $FIRST_F, expected $EXPECTED"
fi

# Every point inside its own interval. Cheap, and it is the assertion that
# catches a band accidentally built from a different series than its points.
BAD=$(jq -r '[.forecast[] | select(.lower != null and (.point < .lower or .point > .upper))] | length' <<<"$NS")
if [ "$BAD" = "0" ]; then
  pass "every forecast point sits inside its own prediction interval"
else
  fail "$BAD forecast bucket(s) fall outside their own interval"
fi

# A refusal is reported, never implied. `banquet_covers` wound down partway
# through the window, so on a 90-day history it has nothing like eight seasonal
# cycles of measured buckets — and must say so rather than return a flat line.
post projection "{\"roots\":[\"store_days.banquet_covers\"],\"time_dimension\":\"$TD\",\
\"period\":$RECENT,\"granularity\":\"month\",\"horizon\":3}" "$TMP/proj_short.json"
SHORT=$(jq -r '.series[0].refusal // "none"' "$TMP/proj_short.json")
SHORT_FC=$(jq -r '.series[0].forecast | length' "$TMP/proj_short.json")
if [ "$SHORT_FC" = "0" ] && [ "$SHORT" != "none" ]; then
  pass "too little history refuses in words, with no forecast: '$SHORT'"
else
  fail "a 3-month history produced $SHORT_FC forecast bucket(s) and refusal '$SHORT' — \
a series under the seasonal-cycle floor must refuse, not extrapolate its own ramp"
fi

# The horizon is capped, and loudly. A silent clamp would return a curve that
# stops early with nothing saying why.
CODE=$(curl -s -X POST "$BASE/projection" -H 'Content-Type: application/json' \
  -d "{\"roots\":[\"store_days.marketing_spend\"],\"time_dimension\":\"$TD\",\
\"period\":$FULL,\"horizon\":5000}" -o /dev/null -w '%{http_code}')
if [ "$CODE" != "200" ]; then
  pass "an out-of-range horizon is refused (HTTP $CODE), not silently clamped"
else
  fail "a horizon of 5000 returned 200 — it must refuse rather than truncate quietly"
fi

echo
if [ "$FAILURES" -eq 0 ]; then
  echo "all scenario cases hold"
else
  echo "$FAILURES failure(s)"
fi
exit $((FAILURES > 0))
