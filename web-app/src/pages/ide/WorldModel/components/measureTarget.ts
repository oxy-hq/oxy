/**
 * The view a world-model measure is declared on.
 *
 * The world model addresses a measure as `(entity, measureName)`, but an
 * *induced* measure is only promoted to that grain — it lives on the view it
 * was promoted from. That source view is where the metric tree knows it as a
 * node, and where its time dimensions are declared.
 */
export function declaringView(
  induced: boolean,
  promotedFrom: string | undefined,
  entityView: string | undefined
): string | undefined {
  return induced ? promotedFrom : entityView;
}

/** Short measure name from a `view.measure` node id (drops the view prefix). */
export function shortMeasureName(id: string): string {
  return id.split(".").slice(1).join(".") || id;
}

/** Compact magnitude for dense mono rows: 1.20M / 412.0k / 87.4. */
export function formatCompact(n: number): string {
  const abs = Math.abs(n);
  if (abs === 0) return "0";
  if (abs >= 1_000_000) return `${(n / 1_000_000).toFixed(2)}M`;
  if (abs >= 1_000) return `${(n / 1_000).toFixed(1)}k`;
  if (abs >= 1) return n.toFixed(1);
  return n.toPrecision(2);
}

/** Explicitly signed delta, so a change reads as a direction, not a level. */
export function formatDelta(n: number): string {
  return `${n >= 0 ? "+" : ""}${formatCompact(n)}`;
}

/**
 * Signed share of `value` against a positive `base`, e.g. `+8%` / `+0.4%`.
 * Gives an absolute upside a relative scale — "how much of the whole is this?".
 * Returns `null` when `base` is non-positive (a percentage would be meaningless
 * or divide-by-zero), so callers can drop the figure rather than print `NaN%`.
 * One decimal below 10% so small-but-real shares don't collapse to `0%`.
 */
export function formatSignedPct(value: number, base: number): string | null {
  if (!Number.isFinite(base) || base <= 0) return null;
  const pct = (value / base) * 100;
  const digits = Math.abs(pct) >= 10 ? 0 : 1;
  const body = pct.toFixed(digits).replace(/\.0$/, "");
  return `${pct >= 0 ? "+" : ""}${body}%`;
}

/**
 * Compact magnitude for whole counts (row counts, volumes): 1.2M / 4.1k / 412.
 * Unlike {@link formatCompact} this never shows a trailing `.0` on an integer —
 * "412 rows", not "412.0 rows".
 */
export function formatCount(n: number): string {
  const abs = Math.abs(n);
  if (abs >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (abs >= 1_000) return `${(n / 1_000).toFixed(1)}k`;
  return Math.round(n).toLocaleString();
}

/**
 * Singular and plural nouns for the rows a view counts, for prose like
 * "533.9 → 801.6 per order · 189 orders".
 *
 * Taken from the VIEW name, not the count measure's: a `type: count` measure
 * counts its view's rows, so a count on `orders` counts orders no matter what
 * the measure is called (`total_orders`, `order_count`, `n`). The measure's own
 * name and description are both unusable here — `total_orders` reads "per
 * total_orders", and its description is a sentence ("Number of orders"), not a
 * noun.
 *
 * Both forms are derived from a normalized singular rather than from the view
 * name as written, because view names are not reliably plural: `orders` and
 * `order` must both yield "per order" / "189 orders". The rules are naive
 * English (drop a trailing "s", add "es" after a sibilant), which is fine here —
 * this is display prose, so a mangled plural is cosmetic and never a wrong
 * number. The exact denominator measure is named in the method tooltip, which is
 * where precision belongs.
 */
export function rowUnit(view: string): { one: string; many: string } {
  const base = view.toLowerCase().replace(/_/g, " ");
  const one = base.endsWith("s") && !base.endsWith("ss") ? base.slice(0, -1) : base;
  const many = one.endsWith("s") ? `${one}es` : `${one}s`;
  return { one, many };
}

/**
 * Elasticity (β) of a driver on its target: the cumulative chain-rule
 * coefficient along the path. `null` means the edge carries no quantitative
 * coefficient — a qualitative driver the tree knows about but cannot size.
 */
export function formatBeta(coefficient: number | null | undefined): string {
  if (coefficient === null || coefficient === undefined) return "—";
  return coefficient.toFixed(2);
}

/** Rank drivers by absolute leverage; unquantified drivers sink to the bottom. */
export function byLeverage<T extends { effective_coefficient?: number | null }>(
  a: T,
  b: T
): number {
  const av = a.effective_coefficient;
  const bv = b.effective_coefficient;
  if (av === null || av === undefined) return bv === null || bv === undefined ? 0 : 1;
  if (bv === null || bv === undefined) return -1;
  return Math.abs(bv) - Math.abs(av);
}
