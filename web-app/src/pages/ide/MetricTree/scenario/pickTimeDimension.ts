import type { LeverInput } from "./resolveLever";

/**
 * The time dimension to baseline a scenario against, or `null` to stay in
 * delta-only mode.
 *
 * Only a dimension on a lever's OWN view can produce a meaningful baseline:
 * a metric tree spans unrelated views, so the first dimension in the layer is
 * as likely to be `astronauts.birth_date` as anything useful — filtering a
 * revenue measure by it makes the warehouse query fail or return nothing.
 *
 * Refusing to guess is the safe outcome: delta-only mode still propagates and
 * still reports every impact, so a wrong pick is strictly worse than no pick.
 *
 * WITHIN the chosen view, `[0]` means FIRST DECLARED — `byView` preserves the
 * order the `.view.yml` lists its dimensions in. That is a real signal and not
 * a coin flip: a `checks` view declaring `opened_at` before `closed_at` is its
 * author saying which one dates a check, and this file has no better basis to
 * choose (it holds names, not types). It is also stable — the same workspace
 * picks the same dimension on every load, and only editing the view changes it.
 *
 * Do NOT "fix" this by sorting. Alphabetical order would prefer `closed_at`
 * over `opened_at`, which is the worse answer arrived at more confidently. If a
 * stronger rule is wanted it has to come from the dimension's TYPE — a `date`
 * business-date column over a `datetime` timestamp — which means carrying types
 * through `/time-dimensions` first.
 *
 * Either way the pick is a default, not a verdict: the panel offers every
 * usable dimension, so a wrong one is a click to correct — see
 * `DistributionPanel`'s `timeDimOverride`.
 */
export function pickTimeDimension(
  levers: LeverInput[],
  byView: Record<string, string[]>
): string | null {
  if (leverViews(levers).size === 0) return null;
  return usableTimeDimensions(levers, byView)[0] ?? null;
}

function leverViews(levers: LeverInput[]): Set<string> {
  return new Set(levers.map((l) => viewOf(l.nodeId)).filter(Boolean));
}

/**
 * The time dimensions that can actually panel these levers: the ones declared
 * on a lever's own view.
 *
 * The auto-pick above refuses to guess across views. Offering the full layer
 * in a picker undoes that refusal by hand, and the failure is silent rather
 * than loud. Grouping a measure by a time dimension from a COARSER view joins
 * that view in on whatever key they share — for `checks` measures under
 * `store_days.business_date`, on `location_id` alone, with nothing tying the
 * check's date to the store-day's. Every check then joins to every store-day
 * of its location, so each cell holds the whole window's total and the series
 * is flat across dates. Downstream that reads as "the driver does not vary
 * within any panel": a fit refusing on 26,280 observations that are really one
 * value repeated 73 times.
 *
 * With nothing pinned there is no lever view to be foreign to, so the whole
 * layer is offered — no query runs until something is pinned anyway.
 */
export function usableTimeDimensions(
  levers: LeverInput[],
  byView: Record<string, string[]>
): string[] {
  const views = leverViews(levers);
  if (views.size === 0) return [...new Set(Object.values(byView).flat())];
  return [...new Set([...views].flatMap((v) => byView[v] ?? []))];
}

/**
 * Whether a lever's view is not the one the window is anchored on — so this
 * lever has no baseline, however healthy the scenario looks around it.
 *
 * The window is ONE dimension and it belongs to ONE view. `usableTimeDimensions`
 * spans every lever's view, which is right for the picker (all of those are
 * legitimate anchors to offer) and wrong as an all-clear: with levers on two
 * views, whichever view wins leaves the other's levers unbaselined, and the
 * dimension still passes the `foreign` check because it does belong to *a*
 * lever's view. That check therefore cannot see this, and the panel showed a
 * bare "no baseline value" with no hint that the window was the cause.
 *
 * Not a mismatch to be fixed by widening the read: the server refuses to carry
 * one view's dimension onto another for the same reason `pickTimeDimension`
 * refuses to pick one — a same-named dimension elsewhere is not the same
 * calendar. The fix an analyst has is to move the window or unpin the lever,
 * which is what the copy says.
 */
export function leverOutsideAnchor(nodeId: string, timeDimension: string | null): boolean {
  if (!timeDimension) return false;
  return viewOf(nodeId) !== viewOf(timeDimension);
}

/** The view half of a `view.measure` (or `view.dimension`) path. */
export function viewOf(path: string): string {
  return path.split(".")[0] ?? "";
}
