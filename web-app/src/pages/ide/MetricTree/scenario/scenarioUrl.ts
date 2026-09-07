import type { BaselineInstance } from "@/types/metricTree";
import type { LeverInput } from "./resolveLever";

export interface ScenarioState {
  levers: LeverInput[];
  periodDays: number;
  timeDimension: string | null;
  instance: BaselineInstance | null;
}

/** Matches `DEFAULT_PRESET_DAYS` in WorldModelOpportunitiesSection. */
export const DEFAULT_PERIOD_DAYS = 90;

export const EMPTY_SCENARIO: ScenarioState = {
  levers: [],
  periodDays: DEFAULT_PERIOD_DAYS,
  timeDimension: null,
  instance: null
};

/**
 * The scenario IS the URL — that is what makes it shareable without a table.
 *
 * A lever is encoded as one repeated `lever` param holding a JSON pair, rather
 * than `nodeId:value`: measure ids and typed values both contain colons and
 * percent signs, and a delimiter scheme would need escaping rules that JSON
 * already has.
 */
export function encodeScenario(state: ScenarioState): URLSearchParams {
  const params = new URLSearchParams();
  for (const lever of state.levers) {
    params.append("lever", JSON.stringify([lever.nodeId, lever.raw]));
  }
  params.set("period", `${state.periodDays}d`);
  if (state.timeDimension) params.set("time_dim", state.timeDimension);
  if (state.instance) {
    params.set("scope", JSON.stringify([state.instance.entity, state.instance.key]));
  }
  return params;
}

/**
 * Never throws. A stale or hand-edited link degrades to whatever parts still
 * parse — a white-screened IDE tab is a far worse outcome than a lost lever.
 */
export function decodeScenario(params: URLSearchParams): ScenarioState {
  const levers: LeverInput[] = [];
  for (const raw of params.getAll("lever")) {
    const pair = safeJsonPair(raw);
    if (pair) levers.push({ nodeId: pair[0], raw: pair[1] });
  }

  const periodMatch = /^(\d+)d$/.exec(params.get("period") ?? "");
  const periodDays = periodMatch ? Number(periodMatch[1]) : DEFAULT_PERIOD_DAYS;

  const scopePair = safeJsonPair(params.get("scope") ?? "");

  return {
    levers,
    periodDays,
    timeDimension: params.get("time_dim"),
    instance: scopePair ? { entity: scopePair[0], key: scopePair[1] } : null
  };
}

/** A JSON `[string, string]`, or null for anything else. */
function safeJsonPair(raw: string): [string, string] | null {
  if (!raw) return null;
  try {
    const parsed = JSON.parse(raw);
    if (
      Array.isArray(parsed) &&
      parsed.length === 2 &&
      typeof parsed[0] === "string" &&
      typeof parsed[1] === "string"
    ) {
      return [parsed[0], parsed[1]];
    }
  } catch {
    // Malformed param — fall through to null.
  }
  return null;
}

/**
 * What a scenario surface may hand back: the next state, or an updater over
 * the latest one.
 *
 * The updater form exists because more than one effect writes whole-scenario
 * state in the same tick — the time-dimension adopt, the vanished-lever drop,
 * and `pinLever`. Spreading a props snapshot means the second write reverts
 * the first's field; `prev => ...` cannot.
 */
export type ScenarioUpdate = ScenarioState | ((prev: ScenarioState) => ScenarioState);
