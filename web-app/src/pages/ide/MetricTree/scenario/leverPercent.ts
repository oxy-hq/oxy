/**
 * Mapping between a lever's typed `raw` string and the slider's position.
 *
 * The slider is always a *percentage* change, which is what lets it work
 * without a baseline: a percentage has a natural range (−100%…+100%) whether
 * or not the measure could be valued, while an absolute range would need a
 * current value the scenario often doesn't have.
 *
 * `raw` stays the single source of truth — the slider writes into it, and
 * `resolveLever` reads it exactly as it does a typed value. Nothing
 * downstream knows a slider exists.
 */

/** Widest move the slider offers. Bigger swings are still typeable. */
export const SLIDER_RANGE = 100;

/**
 * The slider position for `raw`, or `null` when `raw` isn't a percentage
 * (an absolute target like `"11"` or a signed delta like `"+3"`).
 *
 * `null` is not an error: those are legitimate typed values the slider simply
 * can't represent, so the caller parks the handle and lets the text win.
 */
export function percentFromRaw(raw: string): number | null {
  const trimmed = raw.trim();
  if (!trimmed.endsWith("%")) return null;
  const value = Number(trimmed.slice(0, -1).trim());
  return Number.isFinite(value) ? value : null;
}

/**
 * The `raw` string for a slider position. Always explicitly signed so it
 * round-trips through `resolveLever` as a percentage rather than being read
 * as an absolute target.
 */
export function rawFromPercent(percent: number): string {
  const rounded = Math.round(percent);
  return `${rounded > 0 ? "+" : ""}${rounded}%`;
}

/**
 * A lever move expressed as a fraction (`0.5`, `2.0`) rendered as the signed
 * percentage it is: `"+50%"`, `"+200%"`.
 *
 * Not `utils/measureFormat`'s `formatPercent`, which deliberately drops the
 * `%` at a ratio of 1 or more so a plain multiple (items per order ≈ 2.3)
 * can't be misread as a share. That rule is right for a measure *value* and
 * wrong for every number here: these are moves in the lever, so a doubling is
 * `+200%` and never a bare `2.00` — which read as an absolute target sitting
 * next to `-50%`, and left a break-even past +100% unsigned beside a peak that
 * kept its sign.
 */
export function formatLeverPercent(fraction: number): string {
  const pct = fraction * 100;
  const digits = Math.abs(pct) < 1 && pct !== 0 ? 2 : 0;
  return `${pct > 0 ? "+" : ""}${pct.toFixed(digits)}%`;
}
