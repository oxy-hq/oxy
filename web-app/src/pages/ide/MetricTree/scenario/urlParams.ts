import { encodeScenario, type ScenarioState } from "./scenarioUrl";

/**
 * `simulation` is deliberately a peer of the other two rather than a tab inside
 * Scenario. Scenario answers "if I moved this lever, what would the model say";
 * Simulation answers "was the model right, and did following it pay". The
 * second question needs a world whose answer we chose, so it shares the canvas
 * and nothing else — no levers, no time dimension, no scenario state.
 */
export type Mode = "explore" | "scenario" | "simulation";

/** Every query key the scenario URL write-back owns. Anything else already in
 *  the search params (e.g. the IDE tab's `view`) is unrelated and must survive
 *  a write untouched — this list must stay exactly what `encodeScenario` can
 *  produce, plus `mode`. */
const SCENARIO_PARAM_KEYS = ["mode", "lever", "period", "time_dim", "scope"] as const;

/**
 * Merge mode + scenario state into the CURRENT query string — the URL is the
 * single source of truth for both, so a share link reopens exactly this view.
 * Only the scenario-owned keys are replaced; unrelated params are preserved,
 * so writing a scenario doesn't clobber e.g. the parent tab's `view` param
 * and unmount the whole feature.
 */
export function buildParams(
  base: URLSearchParams,
  mode: Mode,
  scenario: ScenarioState
): URLSearchParams {
  const params = new URLSearchParams(base);
  for (const key of SCENARIO_PARAM_KEYS) params.delete(key);
  for (const [key, value] of encodeScenario(scenario).entries()) {
    if (key === "lever") params.append(key, value);
    else params.set(key, value);
  }
  if (mode !== "explore") params.set("mode", mode);
  return params;
}
