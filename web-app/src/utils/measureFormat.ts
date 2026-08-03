/** Rendering of measure values and deltas, for every surface that shows them.
 *
 *  In `src/utils` rather than beside one of them because the surfaces sit in
 *  different feature folders and render the *same* numbers next to each other:
 *  the anomaly inbox table, the explain drawer's driver rows, its Graph tab and
 *  its List tab (`MetricTree/components/ExplainTree`, also the RCA panel's row
 *  renderer), plus the analytics thread's explain artifact. Four local copies of
 *  `formatNumber` had drifted far enough that one Tabs component showed
 *  `Δ +0.00380` on one tab and `+0.00` on the other for the same value.
 *
 *  A formatter whose rounding decides whether a movement is visible at all is
 *  not a per-file detail. */

/** Compact, magnitude-aware rendering of a measure value or delta.
 *
 *  Currency-scale values keep two decimals behind k/M suffixes. Sub-1 values —
 *  rates and shares, where two decimals collapses every value *and every delta
 *  between them* to "0.00" — switch to three significant figures instead:
 *  `0.0964`, `0.100`, `0.00380`.
 *
 *  Below ~1e-6 that yields exponential form (`1.00e-7`). Deliberate: an
 *  awkward-looking tiny number is honest, whereas "0.00" claims nothing
 *  happened. */
export function formatNumber(n: number): string {
  if (Math.abs(n) >= 1_000_000) return `${(n / 1_000_000).toFixed(2)}M`;
  if (Math.abs(n) >= 1_000) return `${(n / 1_000).toFixed(2)}k`;
  if (n !== 0 && Math.abs(n) < 1) return n.toPrecision(3);
  return n.toFixed(2);
}

/** `formatNumber` with an explicit `+` on non-negative values — for deltas
 *  presented side by side, where the sign is the point. */
export function formatSigned(n: number): string {
  return `${n >= 0 ? "+" : ""}${formatNumber(n)}`;
}

/** A ratio (`0.0964`) as a percentage (`"9.64%"`).
 *
 *  Carries the same sub-threshold rule as `formatNumber` for the same reason: a
 *  share small enough to round to "0.00%" is still not zero, and a passthrough
 *  ratio that reads as zero on both sides of the arrow hides the whole move.
 *
 *  A ratio of 1 or more is *not* rendered as a percentage. Nothing guarantees
 *  the two measures in a passthrough pair are a part and its whole — items per
 *  order sits around 2.3 — and "230.00%" invites reading a plain ratio as a
 *  share of something. Those fall back to `formatNumber` and read as the
 *  multiples they are. */
export function formatPercent(n: number): string {
  if (Math.abs(n) >= 1) return formatNumber(n);
  const pct = n * 100;
  if (pct !== 0 && Math.abs(pct) < 0.01) return `${pct.toPrecision(3)}%`;
  return `${pct.toFixed(2)}%`;
}

/** Last segment of a dotted measure reference (`sales.total_discounts` →
 *  `total_discounts`), for the places where the view prefix is noise. */
export function shortMeasureName(measure: string): string {
  return measure.split(".").pop() ?? measure;
}
