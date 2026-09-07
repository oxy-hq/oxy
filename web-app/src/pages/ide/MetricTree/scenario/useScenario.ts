import { useEffect, useMemo, useRef, useState } from "react";
import { useBaseline, usePredict } from "@/hooks/api/useMetricTree";
import type {
  BaselineRequest,
  BaselineResponse,
  FittedDriver,
  MeasureValues,
  MetricNode,
  MetricTree,
  PredictChange,
  PredictImpact,
  UnvaluedReason
} from "@/types/metricTree";
import { type LeverConflict, leverConflicts } from "./leverConflicts";
import type { ScenarioNodeData } from "./nodeValue";
import { type LeverError, resolveLever } from "./resolveLever";
import type { ScenarioState } from "./scenarioUrl";

const DEBOUNCE_MS = 300;

interface UseScenarioArgs {
  tree: MetricTree;
  state: ScenarioState;
}

/**
 * Why the downstream-impact list is empty, when it is.
 *
 * `ran` — propagation happened and found nothing, so "this lever moves no
 *   other modelled measure" is a real finding about the model.
 * `unmoved` — every pinned lever sits at its current value (`no_change`), so
 *   `predict` was never called. Nothing is wrong and nothing was simulated;
 *   this is the state a freshly pinned lever and a centred slider are in.
 * `unresolved` — at least one input could not be read as a value, so the run
 *   was blocked by something the analyst can fix.
 */
export type ScenarioRunState = "ran" | "unmoved" | "unresolved";

export interface UseScenarioResult {
  nodeData: Map<string, ScenarioNodeData>;
  conflicts: LeverConflict[];
  leverErrors: Map<string, LeverError>;
  baselineError: Error | null;
  /** A failed propagation. Surfaced because the alternative — a silently
   *  inert canvas — is indistinguishable from "this lever moves nothing". */
  propagationError: Error | null;
  /** The server's explanation for what the baseline could not value. Shown
   *  verbatim: it names which of the causes actually occurred, which a
   *  client-side guess never could. */
  baselineNote: string | null;
  /** Whether the baseline valued anything at all.
   *
   *  Separate from `baselineNote` because the two are independent: the server
   *  composes the note from the engine outcome AND the list of views it never
   *  got to read, so a read that valued one view and skipped another carries
   *  a note next to a full set of usable baselines. Reading the note alone as
   *  "there is no baseline" told an analyst that only signed deltas would
   *  resolve while a % lever was resolving on screen beside the sentence. */
  anyValued: boolean;
  /** Coefficients the baseline measured from history, and its refusals.
   *  Surfaced because a refusal is the only explanation for a branch of the
   *  canvas showing nothing — without it the UI is silent about its own
   *  silence. */
  fitted: FittedDriver[];
  unreachableCount: number;
  /** Why the impact list is empty, when it is. Only this hook can tell the
   *  three apart, and they need three different sentences. */
  runState: ScenarioRunState;
}

/** `n` days before today, as `YYYY-MM-DD` (UTC). */
function daysAgoIso(n: number): string {
  const d = new Date();
  d.setUTCDate(d.getUTCDate() - n);
  return d.toISOString().slice(0, 10);
}

/**
 * Trailing window of `days`, ending yesterday — today is excluded because a
 * partial day would read as a depressed baseline. Mirrors `presetPeriod` in
 * `WorldModelOpportunitiesSection`; kept local rather than imported so the
 * Metric Tree scenario module doesn't reach into the World Model feature.
 */
function periodFromDays(days: number): [string, string] {
  return [daysAgoIso(days), daysAgoIso(1)];
}

/**
 * Baseline request for the current lever set, or `null` to disable the
 * query — a conflicting or empty lever set must not spend a warehouse query.
 */
function buildBaselineRequest(blocked: boolean, state: ScenarioState): BaselineRequest | null {
  if (blocked || state.levers.length === 0 || !state.timeDimension) return null;
  return {
    roots: state.levers.map((l) => l.nodeId),
    time_dimension: state.timeDimension,
    period: periodFromDays(state.periodDays),
    instance: state.instance
  };
}

/**
 * Resolve typed lever values into deltas against the baseline. `no_change`
 * is not surfaced as an error — it just means there is nothing to propagate.
 *
 * `leverDeltas` is the same set of resolved deltas keyed by node, kept because
 * a lever's own move is knowable ONLY here: `predict` reports what a change
 * causes, never the change itself, so nothing downstream can reconstruct it.
 */
function useLeverResolution(
  levers: ScenarioState["levers"],
  baselineData: BaselineResponse | undefined
): {
  leverErrors: Map<string, LeverError>;
  changes: PredictChange[];
  leverDeltas: Map<string, number>;
  runState: ScenarioRunState;
} {
  const resolved = useMemo(
    () => levers.map((l) => resolveLever(l, baselineData?.values ?? {})),
    [levers, baselineData]
  );
  const leverErrors = useMemo(() => {
    const errors = new Map<string, LeverError>();
    for (const r of resolved) {
      if ("error" in r && r.error !== "no_change") errors.set(r.nodeId, r.error);
    }
    return errors;
  }, [resolved]);
  const changes = useMemo(
    () => resolved.flatMap((r) => ("delta" in r ? [{ measure: r.nodeId, delta: r.delta }] : [])),
    [resolved]
  );
  const leverDeltas = useMemo(() => new Map(changes.map((c) => [c.measure, c.delta])), [changes]);
  // Three states, not two. `useDebouncedPredict` bails on `changes.length ===
  // 0`, so "every input is fine" does NOT imply propagation ran: a lever left
  // at its current value resolves to `no_change`, which is neither an error
  // nor a simulation. Collapsing that into "resolved" made an empty impact
  // list read as "this lever moves no other modelled measure" — a modelling
  // claim about a scenario that was never run, which is the exact thing this
  // flag exists to prevent.
  const runState: ScenarioRunState =
    changes.length > 0 ? "ran" : leverErrors.size > 0 ? "unresolved" : "unmoved";
  return { leverErrors, changes, leverDeltas, runState };
}

interface PredictResult {
  impacts: PredictImpact[] | undefined;
  error: Error | null;
}

const IDLE_PREDICT_RESULT: PredictResult = { impacts: undefined, error: null };

/**
 * Propagate the current changes, debounced. Mirrors WorldModelWhatIf's
 * existing 300ms. A blocked or empty change set resets the mutation instead
 * of firing, so a stale prediction never lingers under a new conflict.
 *
 * A baseline is not required to propagate: without one (no time dimension in
 * this layer), a signed-delta lever still resolves to a real delta, and
 * `predict` runs additive/component edges exactly with no `values` — only
 * multiplicative edges come back `unquantifiable`. Omitting `values` (rather
 * than passing `{}`) is what tells the endpoint this is delta-only mode.
 *
 * The baseline's fitted coefficients ride along for the same reason `values`
 * does: fitting is a warehouse query keyed on (levers, period, scope), while
 * this re-runs on every keystroke. Re-measuring here would put a regression
 * behind each character typed.
 *
 * `usePredict` is a mutation, not a query — unlike `useBaseline`, its shared
 * `data`/`error` are not keyed by input, so nothing about them says which
 * request produced them. Two calls in flight together (a lever moved twice
 * before the first answer lands) settle in whatever order the network
 * delivers them, and the mutation object just reflects whichever one settled
 * LAST. A slow, already-superseded response landing after a fast, current one
 * would silently overwrite it — numbers for a scenario the user already moved
 * past. `requestIdRef` guards against exactly that: each fired request is
 * stamped with a token, and its `onSuccess`/`onError` is only applied if that
 * token is still the latest — a later edit (or a block/reset) bumps the ref
 * first, so a superseded response is dropped on arrival instead of being
 * allowed to overwrite `impacts`/`error`.
 */
function useDebouncedPredict(
  blocked: boolean,
  changes: PredictChange[],
  baselineData: BaselineResponse | undefined,
  predict: ReturnType<typeof usePredict>
): PredictResult {
  const { mutate: runPredict, reset: resetPredict } = predict;
  const [result, setResult] = useState<PredictResult>(IDLE_PREDICT_RESULT);
  const requestIdRef = useRef(0);

  useEffect(() => {
    if (blocked || changes.length === 0) {
      requestIdRef.current++;
      resetPredict();
      setResult(IDLE_PREDICT_RESULT);
      return;
    }
    const values = baselineData?.values;
    const coefficients = baselineData?.fitted;
    const handle = window.setTimeout(() => {
      const requestId = ++requestIdRef.current;
      runPredict(
        { changes, ...(values ? { values } : {}), coefficients },
        {
          onSuccess: (data) => {
            // Superseded by a later edit while this was in flight — a stale
            // result must not clobber the current one, or set/clear it either.
            if (requestId !== requestIdRef.current) return;
            setResult({ impacts: data.impacts, error: null });
          },
          onError: (error) => {
            if (requestId !== requestIdRef.current) return;
            setResult({ impacts: undefined, error });
          }
        }
      );
    }, DEBOUNCE_MS);
    return () => window.clearTimeout(handle);
  }, [blocked, changes, baselineData, runPredict, resetPredict]);

  return result;
}

/**
 * Orchestrates a scenario: conflict check → baseline → propagate.
 *
 * The two network calls are deliberately separate. The baseline is a warehouse
 * query keyed on (levers, period, scope) and refetches only when one of those
 * changes; propagation is a pure server-side graph walk, so editing a lever's
 * VALUE re-runs only that, debounced to coalesce keystrokes.
 */
export function useScenario({ tree, state }: UseScenarioArgs): UseScenarioResult {
  // 1. Conflicts first — a conflicting set issues no request at all.
  const conflicts = useMemo(
    () =>
      leverConflicts(
        tree,
        state.levers.map((l) => l.nodeId)
      ),
    [tree, state.levers]
  );
  const blocked = conflicts.length > 0;

  // 2. Baseline.
  const baselineRequest = useMemo(() => buildBaselineRequest(blocked, state), [blocked, state]);
  const baseline = useBaseline(baselineRequest);

  // 3. Resolve what was typed into deltas.
  const { leverErrors, changes, leverDeltas, runState } = useLeverResolution(
    state.levers,
    baseline.data
  );

  // 4. Propagate, debounced. `predictResult` is fed only by a response that
  //    still answers the current `changes` — see `useDebouncedPredict`.
  const predict = usePredict();
  const predictResult = useDebouncedPredict(blocked, changes, baseline.data, predict);

  // 5. Fold everything into per-node render data. Blocked keeps the last
  //    non-blocked result — the design shows it greyed under the banner
  //    rather than blanking the canvas.
  const computed = useMemo(
    () =>
      buildNodeData({
        tree,
        state,
        baseline: baseline.data,
        impacts: predictResult.impacts,
        leverDeltas
      }),
    [tree, state, baseline.data, predictResult.impacts, leverDeltas]
  );
  const lastNodeData = useRef(computed);
  if (!blocked) lastNodeData.current = computed;
  const nodeData = blocked ? lastNodeData.current : computed;

  let unreachableCount = 0;
  for (const data of nodeData.values()) {
    if (data.state === "unreachable") unreachableCount++;
  }

  return {
    nodeData,
    conflicts,
    leverErrors,
    baselineError: baseline.error,
    propagationError: predictResult.error,
    baselineNote: baseline.data?.baseline_note ?? null,
    anyValued: Object.keys(baseline.data?.values ?? {}).length > 0,
    fitted: baseline.data?.fitted ?? [],
    unreachableCount,
    runState
  };
}

interface BuildNodeDataArgs {
  tree: MetricTree;
  state: ScenarioState;
  baseline?: BaselineResponse;
  impacts?: PredictImpact[];
  /** Each lever's own resolved delta, from `useLeverResolution`. */
  leverDeltas: Map<string, number>;
}

/**
 * Fold the tree, the typed levers, the baseline and the propagated impacts
 * into one render-ready record per node. State-assignment precedence is
 * deliberate and fixed: lever → impacted/unquantifiable → unvalued →
 * unchanged → unreachable — a node can only be one thing at a time. An impact
 * outranks a missing baseline: the delta is knowable without one, so a failed
 * baseline must not silently reclassify a measure the lever genuinely moved.
 */
function buildNodeData({
  tree,
  state,
  baseline,
  impacts,
  leverDeltas
}: BuildNodeDataArgs): Map<string, ScenarioNodeData> {
  const leverRaw = new Map(state.levers.map((l) => [l.nodeId, l.raw]));
  const unvaluedReasons = new Map((baseline?.unvalued ?? []).map((u) => [u.node_id, u.reason]));
  const impactByMeasure = new Map((impacts ?? []).map((i) => [i.measure, i]));
  const values = baseline?.values ?? {};
  // In delta-only mode (no time dimension ⇒ no baseline) `values` and
  // `unvaluedReasons` are both empty, so a genuinely-impacted downstream node
  // must be counted reachable via `impacts` alone, or it folds to
  // "unreachable" instead of surfacing the propagated result.
  const reachable = new Set<string>([
    ...Object.keys(values),
    ...unvaluedReasons.keys(),
    ...impactByMeasure.keys()
  ]);

  const result = new Map<string, ScenarioNodeData>();
  for (const node of tree.nodes) {
    result.set(
      node.id,
      buildOneNode(node, {
        leverRaw,
        leverDeltas,
        unvaluedReasons,
        impactByMeasure,
        values,
        reachable
      })
    );
  }
  return result;
}

interface NodeContext {
  leverRaw: Map<string, string>;
  leverDeltas: Map<string, number>;
  unvaluedReasons: Map<string, UnvaluedReason>;
  impactByMeasure: Map<string, PredictImpact>;
  values: MeasureValues;
  reachable: Set<string>;
}

function buildOneNode(node: MetricNode, ctx: NodeContext): ScenarioNodeData {
  const { leverRaw, leverDeltas, unvaluedReasons, impactByMeasure, values, reachable } = ctx;
  const raw = values[node.id];
  const baselineValue = Number.isFinite(raw) ? raw : undefined;

  // A lever carries its OWN move, not just the value it started from. The delta
  // comes from `resolveLever`, never from `impacts` — the engine reports what a
  // change causes and would have to list the lever as its own consequence to
  // report it here. `undefined` when the lever errored or resolved to
  // `no_change`, which must stay distinct from a delta of 0.
  const typedLever = leverRaw.get(node.id);
  if (typedLever !== undefined) {
    const delta = leverDeltas.get(node.id);
    return {
      node,
      state: "lever",
      baseline: baselineValue,
      simulated:
        baselineValue !== undefined && delta !== undefined ? baselineValue + delta : undefined,
      delta,
      leverRaw: typedLever
    };
  }

  // An impact outranks a missing baseline. The propagated delta is knowable
  // WITHOUT a baseline, so a node the baseline query couldn't value is still
  // a genuine, reportable impact — ranking `unvalued` first meant a failed
  // baseline silently reclassified every moved measure and the panel then
  // claimed the lever moved nothing.
  const impact = impactByMeasure.get(node.id);
  const unvaluedReason = unvaluedReasons.get(node.id);

  if (impact?.confidence === "unquantifiable") {
    return {
      node,
      state: "unquantifiable",
      baseline: baselineValue,
      confidence: impact.confidence,
      // Carried even with no number to explain: the path is what names the edge
      // the model could not size, which is the whole content of this state.
      // `impact.form` is NOT carried — it is a hardcoded `linear` placeholder on
      // an unquantifiable impact, and rendering it would invent a shape nothing
      // fitted. Per-hop forms come from the tree edges instead.
      path: impact.path,
      lag: impact.lag
    };
  }

  if (!impact && unvaluedReason) {
    return { node, state: "unvalued", baseline: baselineValue, unvaluedReason };
  }

  if (!reachable.has(node.id)) {
    return { node, state: "unreachable" };
  }

  // Reachable, valued, but nothing propagated to it. This is its own state,
  // not an impact with a missing number: the baseline values everything
  // forward-reachable from a lever, so every node downstream of an edge the
  // model could not size lands here. Filing them as "impacted" left the
  // canvas full of highlighted nodes with empty bodies and an impact list of
  // blank rows — the surface claiming a move it could not name.
  if (!impact) {
    return { node, state: "unchanged", baseline: baselineValue };
  }

  const simulated =
    baselineValue !== undefined ? baselineValue + impact.estimated_delta : undefined;
  return {
    node,
    state: "impacted",
    baseline: baselineValue,
    simulated,
    // Carried even when a baseline exists, but load-bearing without one: in
    // delta-only mode this is the only number there is, and dropping it left
    // the node with nothing to render.
    delta: impact.estimated_delta,
    confidence: impact.confidence,
    path: impact.path,
    lag: impact.lag
  };
}
