import type { FittedDriver } from "@/types/metricTree";

/** Where a curvature stops being "only just resolved".
 *
 *  The engine's own bar is |t| >= 2, so nothing below it ever arrives here and
 *  this can never say "not significant". One point of headroom above that bar
 *  is the line: it is deliberately a display threshold rather than a second
 *  statistical claim, and it exists to separate a fit that would survive a
 *  moved window from one whose peak is a coin-flip away from vanishing. On the
 *  project's fixture the curvature comes in at |t| 33.5, far clear of it.
 *
 *  Not a gate — both sides render a peak. It only changes how loudly. */
const CURVATURE_COMFORTABLE = 3;

/**
 * How well determined the turn is, from the curvature term's own t-statistic.
 *
 * A peak is not a measurement — it is `vertex·s₁/s₂ − 1`, a ratio built from a
 * difference of two close quantities, so a curvature that barely cleared the
 * gate moves the reported peak a long way. `t_stats` carries a t PER BASIS
 * TERM, and `[1]` is the SECOND basis term: the squared one under every shape
 * that can turn (`[x, x²]`, `[x, x², x³]`, `[ln x, (ln x)²]`), which is what
 * makes one index right for all of them. The headline `t_stat` is the slope's
 * and says nothing about whether the shape turns at all.
 *
 * `se_terms` is deliberately left unread: `t = β/se`, so it carries the same
 * information in units the reader would have to divide out themselves.
 *
 * Read beside a peak AND beside a break-even without one: both are read off the
 * same fitted shape, so a crossing is as sensitive to a marginal curvature as a
 * turn is. A single-term form has no `[1]` to find and gets nothing. On a
 * `cubic` the turn also depends on the third term; this is the leading
 * contribution, not the whole story, which is why the copy hedges rather than
 * quantifies.
 */
export function curvatureNote(fit: FittedDriver): string {
  const curvature = fit.t_stats?.[1];
  if (curvature == null || !Number.isFinite(curvature)) return "";
  const magnitude = Math.abs(curvature);
  return magnitude < CURVATURE_COMFORTABLE
    ? ` The curvature is only just resolved (|t| ${magnitude.toFixed(1)}), so the turning point is approximate and can move as the window does.`
    : ` The curvature is well resolved (|t| ${magnitude.toFixed(1)}).`;
}
