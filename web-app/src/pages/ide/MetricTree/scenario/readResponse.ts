import type { FittedDriver } from "@/types/metricTree";

/**
 * What a fitted driver response actually does, read off the curve.
 *
 * This replaces a `switch` on `form` plus a closed-form vertex solver, and the
 * reason is worth keeping: both had to be extended for every shape. The switch
 * needed a sentence per form to say what unit its coefficient was in; the solver
 * only ever worked for `[x, x²]`, so a cubic or a spline that also turned over
 * would have silently reported no ceiling.
 *
 * The engine now ships the response **sampled** (`fitted.profile`, as
 * `[lever fraction, delta]` pairs over the range it has evidence for). Every
 * question the panel asks is a property of those samples:
 *
 * - does it help? → the sign of the delta
 * - where is it best? → the largest sample
 * - where does it stop paying? → the sign change
 * - is it saturating? → a shrinking step
 * - what unit is the coefficient in? → **nobody needs to know.** "+10% → +1,838"
 *   is stated in the measure's own terms.
 *
 * So nothing here mentions a shape, and a shape added to the engine needs no
 * change in this file.
 */
export interface ResponseReading {
  /** Lever fraction of the best sampled outcome, when it is an interior peak
   *  rather than the edge of the sampled range, AND that outcome is an
   *  improvement. `undefined` for a response that simply keeps rising (no
   *  ceiling to report) and for one whose best point is still a loss (nothing
   *  to recommend). */
  peak?: number;
  /** The delta at `peak`. */
  peakDelta?: number;
  /** Lever fraction where the response crosses back through zero: beyond here it
   *  takes the target below where it started. */
  breakEven?: number;
  /** Whether each further step buys less than the last, over the rising part. A
   *  saturating response and a straight line are both monotone, so this is what
   *  distinguishes them.
   *
   *  Only ever true of a response that RISES. A monotone decline is monotone
   *  too, and its steps shrink by the same arithmetic (two equal steps of −10
   *  satisfy `last < first × 0.9`), so without a sign guard the panel told the
   *  analyst "each further increase buys less than the last" about a lever
   *  that was actively lowering the target. */
  saturating: boolean;
  /** Whether pushing this lever LOWERS the target throughout the sampled
   *  range. Distinct from `saturating` — diminishing returns and outright harm
   *  are opposite findings, and only one of them is a reason to keep going. */
  declining: boolean;
  /** A few sampled points worth showing, in the measure's own units. */
  samples: { lever: number; delta: number }[];
}

/**
 * Fit a parabola through the three samples bracketing the maximum and return its
 * vertex.
 *
 * Sub-step accuracy without knowing the shape: exact when the response really is
 * quadratic, and O(h³) for anything else smooth. This is the piece that lets a
 * sampled curve quote a peak as precisely as a solver would, without being a
 * solver for one particular basis.
 */
function refinePeak(
  before: [number, number],
  at: [number, number],
  after: [number, number]
): number {
  const [x0, y0] = before;
  const [x1, y1] = at;
  const [x2, y2] = after;
  const d = (x0 - x1) * (x0 - x2) * (x1 - x2);
  if (d === 0) return x1;
  // Lagrange form of the quadratic through the three points, differentiated.
  const a = (x2 * (y1 - y0) + x1 * (y0 - y2) + x0 * (y2 - y1)) / d;
  const b = (x2 * x2 * (y0 - y1) + x1 * x1 * (y2 - y0) + x0 * x0 * (y1 - y2)) / d;
  if (a === 0) return x1;
  const vertex = -b / (2 * a);
  // Refinement must stay inside the bracket; outside it the parabola is not a
  // description of this curve any more.
  return vertex > x0 && vertex < x2 ? vertex : x1;
}

/** Linear interpolation of the zero crossing between two straddling samples. */
function crossingBetween(a: [number, number], b: [number, number]): number {
  const span = a[1] - b[1];
  return span === 0 ? b[0] : a[0] + ((b[0] - a[0]) * a[1]) / span;
}

export function readResponse(fit: FittedDriver): ResponseReading {
  const profile = (fit.profile ?? []).map(([lever, delta]) => [lever, delta] as [number, number]);
  // Only levers that INCREASE the driver: a scenario panel is asking "what if we
  // pushed this", and including cuts would put the maximum at whichever end
  // happened to be higher.
  const up = profile.filter(([lever]) => lever > 0).sort((a, b) => a[0] - b[0]);
  if (up.length < 3) {
    return { saturating: false, declining: false, samples: [] };
  }

  let maxIdx = 0;
  for (let i = 1; i < up.length; i++) {
    if (up[i][1] > up[maxIdx][1]) maxIdx = i;
  }

  // A maximum at the edge of the sampled range is not a ceiling — it is just the
  // largest lever we were willing to evaluate. Reporting it as "best at +200%"
  // would invent a recommendation out of where the sampling stopped.
  //
  // The delta must also be POSITIVE. A curve of -100, -50, -80 has an interior
  // maximum at -50, and "Best around +10%" for a lever that lowers the target
  // at every sampled point is the same wrong claim `declining` was added to
  // kill — the least-bad point on a harmful curve is not a recommendation.
  const interior = maxIdx > 0 && maxIdx < up.length - 1 && up[maxIdx][1] > 0;
  const peak = interior ? refinePeak(up[maxIdx - 1], up[maxIdx], up[maxIdx + 1]) : undefined;

  // Every place the curve crosses the baseline, not just the first. `cubic` can
  // turn twice, so it can drop through zero and come back — and "past +40% it
  // stops paying for itself" is a false statement about a curve that pays again
  // at +90%. One crossing is a break-even; several are a shape, and the profile
  // beside this is what shows a shape. So report the number only when it is the
  // only one, and say nothing rather than something the curve contradicts.
  const crossings: number[] = [];
  for (let i = 0; i < up.length - 1; i++) {
    const [, here] = up[i];
    const [, next] = up[i + 1];
    if ((here > 0 && next <= 0) || (here <= 0 && next > 0)) {
      crossings.push(crossingBetween(up[i], up[i + 1]));
    }
  }
  const breakEven = crossings.length === 1 && up[0][1] > 0 ? crossings[0] : undefined;

  // Compare the first step against the last one on the way up. Both a line and a
  // saturating curve rise; only the curve's steps shrink.
  const rising = up.slice(0, Math.max(2, interior ? maxIdx + 1 : up.length));
  const firstStep = rising[1][1] - rising[0][1];
  const lastStep = rising[rising.length - 1][1] - rising[rising.length - 2][1];
  // `firstStep > 0` is the guard that makes this mean what it says. Without
  // it the comparison is satisfied by any decline, since a step of −10
  // followed by another −10 is "less than 0.9 × the first".
  //
  // It is not sufficient on its own, because a step can be positive *within* a
  // curve that is negative everywhere: −100, −50, −80 gives `firstStep = +50`
  // and `lastStep = −30`, and "each further increase buys less than the last"
  // is the wrong sentence for a lever that lowers the target at every sampled
  // point. Diminishing returns presupposes returns, so require at least one
  // sampled point to actually be above baseline.
  const anyGain = up.some(([, delta]) => delta > 0);
  // Comparing two steps cannot tell diminishing returns from an S-curve: a
  // `cubic` that is flat, then steep, then flat again has a small first step
  // and a small last one, so the two-point test calls it saturating while the
  // sentence — "each further increase buys less than the last" — is false right
  // through the steep middle. Diminishing returns means the steps never grow,
  // which is a statement about all of them: the first step has to be the
  // largest. On an S-curve the largest step is in the interior, so it fails,
  // and the profile is left to show the shape instead.
  const steps = rising.slice(1).map((point, i) => point[1] - rising[i][1]);
  const stepsNeverGrow = steps.every((step) => step <= firstStep);
  const saturating =
    rising.length > 2 && anyGain && firstStep > 0 && stepsNeverGrow && lastStep < firstStep * 0.9;
  // Never better than where it started, anywhere in the sampled range: the
  // lever does not have a ceiling, it has a cost.
  const declining = !saturating && up.every(([, delta]) => delta <= 0) && up.some(([, d]) => d < 0);

  // A handful of points, chosen so the reader sees the behaviour rather than the
  // sampling grid: a small move, the peak if there is one, and the far end.
  const pick = [up[0], interior ? up[maxIdx] : undefined, up[up.length - 1]];
  return {
    peak,
    peakDelta: interior ? up[maxIdx][1] : undefined,
    breakEven,
    saturating,
    declining,
    samples: pick
      .filter((p): p is [number, number] => p !== undefined)
      .map(([lever, delta]) => ({ lever, delta }))
  };
}
