import type { ProjectionGranularity, ProjectionRequest } from "@/types/metricTree";
import type { ScenarioState } from "./scenarioUrl";

/**
 * How much history each granularity asks for, and how far forward it projects
 * by default.
 *
 * The history window is NOT the scenario's `periodDays`, and that is the whole
 * point of this module. A scenario baseline routinely averages over 30 days;
 * the forecaster refuses anything under eight seasonal cycles
 * (`gates::min_history_buckets` — 56 daily buckets, 32 weekly, 24 monthly), so
 * reusing the scenario window would make "no forecast" the normal answer for
 * the most common preset. Each window below clears its floor several times
 * over, which is what leaves room for the gaps a real warehouse has.
 */
const PROFILE: Record<
  ProjectionGranularity,
  { historyDays: number; defaultHorizon: number; maxHorizon: number }
> = {
  day: { historyDays: 365, defaultHorizon: 30, maxHorizon: 180 },
  week: { historyDays: 730, defaultHorizon: 12, maxHorizon: 52 },
  month: { historyDays: 1460, defaultHorizon: 6, maxHorizon: 24 }
};

export const GRANULARITIES: ProjectionGranularity[] = ["day", "week", "month"];

export function defaultHorizon(granularity: ProjectionGranularity): number {
  return PROFILE[granularity].defaultHorizon;
}

export function horizonChoices(granularity: ProjectionGranularity): number[] {
  const { defaultHorizon: base, maxHorizon } = PROFILE[granularity];
  return [Math.round(base / 2), base, base * 2, maxHorizon].filter(
    (n, i, all) => n >= 1 && n <= maxHorizon && all.indexOf(n) === i
  );
}

/** `n` days before today, as `YYYY-MM-DD` (UTC). */
function daysAgoIso(n: number): string {
  const d = new Date();
  d.setUTCDate(d.getUTCDate() - n);
  return d.toISOString().slice(0, 10);
}

/**
 * The projection request for a scenario, or `null` to disable the query.
 *
 * `null` whenever a curve could not be honest: a conflicting lever set, no
 * lever at all, or no time dimension — a projection is a claim about time, and
 * delta-only mode has no time axis to make it on.
 */
export function buildProjectionRequest(
  blocked: boolean,
  state: ScenarioState,
  granularity: ProjectionGranularity,
  horizon: number
): ProjectionRequest | null {
  if (blocked || state.levers.length === 0 || !state.timeDimension) return null;
  return {
    roots: state.levers.map((l) => l.nodeId),
    time_dimension: state.timeDimension,
    // Ends yesterday, like the scenario's own window: today is partial, and a
    // half-finished bucket at the end of the history reads to the fit as a
    // collapse and bends the forecast down through it.
    period: [daysAgoIso(PROFILE[granularity].historyDays), daysAgoIso(1)],
    instance: state.instance,
    granularity,
    horizon
  };
}
