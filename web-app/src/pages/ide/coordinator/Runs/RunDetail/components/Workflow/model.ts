import type { DagStepSummary, RunEventEntry } from "@/services/api/coordinator";
import { buildWaterfall, type WaterfallModel } from "../Waterfall/model";

/**
 * Workflow-run derived model. The backend gives us:
 *   - `dag_steps` — per-step status / timing reconstructed from
 *     `subrun_step_started` + `subrun_step_completed` event pairs.
 *   - `event_log` — every structural event for the run in seq order.
 *
 * Here we pair the two: for each step, slice the events that fired
 * between its start and end, then inspect that slice to figure out the
 * step's *kind* (sql / agent / generic) and what to surface in the
 * inspector. Linear v1 — workflows declare `depends_on` in YAML but
 * we render execution order as a straight chain; structural edges are
 * a separate enrichment for when procedures grow real fan-outs.
 */
export interface WorkflowNode {
  /** Stable position-based React key — `"0/1/2"` for the third
   *  grandchild of the second child of the root, etc. Survives loop
   *  iterations with duplicate names. */
  key: string;
  /** Step name from the workflow YAML. */
  name: string;
  /** "succeeded" | "failed" | "cached" | "running" | "pending" — pending
   *  is for tree leaves declared in `subrun_started` that haven't
   *  fired a start event yet (e.g. a downstream step that hasn't run
   *  because an earlier step failed). */
  status: string;
  /** Derived from the events that fired during this step's window.
   *  For container nodes (loop / sub-workflow) the kind reflects the
   *  container itself, not its children. */
  kind: WorkflowNodeKind;
  /** Raw `task_type` from the YAML (e.g. `"execute_sql"`,
   *  `"loop_sequential"`, `"workflow"`, `"agent"`). Drives container
   *  detection and the per-type icon. */
  taskType: string;
  /** Wall-clock duration in ms (null while in flight / pending). */
  durationMs: number | null;
  startedAt: string | null;
  completedAt: string | null;
  error?: string;
  cached: boolean;
  /** Filtered events that fired during this step's seq window. */
  events: RunEventEntry[];
  /** When the step delegated to another run (agent / sub-procedure),
   *  the resulting nested waterfall. Recursive — same model the agent
   *  run detail uses, so we render it with the same component. */
  nestedWaterfall?: WaterfallModel;
  /** When the step ran a single SQL query, the captured payload. */
  query?: {
    sql: string;
    rowCount: number;
    success: boolean;
    source: string;
    columns: string[];
    rowsPreview: unknown[][];
    error?: string;
  };
  /** Tree children — populated for container task types
   *  (`loop_sequential`, `workflow`). Empty for leaf steps. */
  children: WorkflowNode[];
}

export type WorkflowNodeKind = "sql" | "agent" | "procedure" | "loop" | "generic";

/** Container task types from the workflow YAML — their `inner_tasks`
 *  become tree children rather than executable leaves. */
const CONTAINER_TYPES = new Set(["loop_sequential", "workflow"]);

/** Raw subrun-step descriptor from the `subrun_started` event payload.
 *  Mirrors `agentic_core::subrun::SubrunStep`. */
interface SubrunStepDesc {
  name: string;
  task_type: string;
  inner_tasks?: SubrunStepDesc[];
}

export interface WorkflowModel {
  nodes: WorkflowNode[];
  /** Index of the critical node within the top-level `nodes` list.
   *  -1 when the critical node lives deeper in the tree (a leaf
   *  inside a container) — read `criticalNode` instead in that case. */
  criticalIndex: number;
  /** The slowest *leaf* across the entire tree. Drives the critical
   *  badge regardless of nesting depth. */
  criticalNode: WorkflowNode | null;
  /** Sum of measured step durations — a rough total even when the
   *  parent run row's updated_at hasn't been stamped yet. */
  totalDurationMs: number;
  /** Shared time window across every measurable step in the tree —
   *  used by the graph to position bars on a common axis. `null` when
   *  no step has a usable `startedAt` yet (very-new runs, fully
   *  synthetic iteration trees). */
  window: { t0Ms: number; t1Ms: number; spanMs: number } | null;
}

const STARTED = "subrun_step_started";
const COMPLETED = "subrun_step_completed";

const num = (v: unknown): number => (typeof v === "number" ? v : 0);
const str = (v: unknown): string => (typeof v === "string" ? v : "");

/** Partition the event log so each step gets the events that fired
 *  while it was the active step — bounded by the matching
 *  subrun_step_started / subrun_step_completed pair. */
const eventsForStep = (events: RunEventEntry[], stepName: string): RunEventEntry[] => {
  // Find the *first* unmatched start for this step name, then take
  // everything up to its matching completed event. This handles loops
  // that fire the same step name multiple times by reading the first
  // occurrence (good enough for v1; the dag_steps row we pair with is
  // also the first occurrence).
  let startSeq: number | null = null;
  let endSeq: number | null = null;
  for (const e of events) {
    if (startSeq === null) {
      if (e.event_type === STARTED && str(e.payload.step) === stepName) {
        startSeq = e.seq;
      }
    } else if (e.event_type === COMPLETED && str(e.payload.step) === stepName) {
      endSeq = e.seq;
      break;
    }
  }
  if (startSeq === null) return [];
  const startBoundary = startSeq;
  const endBoundary = endSeq;
  // Include events strictly between the two boundaries (exclusive of
  // the start/complete markers themselves — those are framing, not
  // content).
  return events.filter((e) => {
    if (e.seq <= startBoundary) return false;
    if (endBoundary !== null && e.seq >= endBoundary) return false;
    return true;
  });
};

/** Map the YAML's `task_type` to one of our four visual kinds. Container
 *  task types are mapped by structure, not events. Event-driven
 *  detection still wins as a fallback when `task_type` is missing or
 *  unknown (older runs predating the `subrun_started` payload). */
const kindForTaskType = (taskType: string): WorkflowNodeKind | null => {
  switch (taskType) {
    case "execute_sql":
    case "semantic_query":
    case "omni_query":
    case "looker_query":
      return "sql";
    case "agent":
      return "agent";
    case "loop_sequential":
      return "loop";
    case "workflow":
      return "procedure";
    default:
      return null;
  }
};

/** Event-driven kind detection — the original v1 logic. Used when the
 *  YAML task_type is missing or unhelpful (e.g. older runs). */
const detectKindFromEvents = (events: RunEventEntry[]): WorkflowNodeKind => {
  let hasDelegation = false;
  let hasQuery = false;
  let hasLlmEnd = false;
  for (const e of events) {
    if (e.event_type === "delegation_started") hasDelegation = true;
    else if (e.event_type === "query_executed" || e.event_type === "execution_failed")
      hasQuery = true;
    else if (e.event_type === "llm_end") hasLlmEnd = true;
  }
  if (hasDelegation) return "procedure";
  if (hasLlmEnd) return "agent";
  if (hasQuery) return "sql";
  return "generic";
};

/** Extract a single query-execution payload when the step ran one. */
const extractQuery = (events: RunEventEntry[]): WorkflowNode["query"] | undefined => {
  const q = events.find((e) => e.event_type === "query_executed");
  if (!q) {
    const fail = events.find((e) => e.event_type === "execution_failed");
    if (!fail) return undefined;
    return {
      sql: str(fail.payload.query),
      rowCount: 0,
      success: false,
      source: str(fail.payload.source) || "Llm",
      columns: [],
      rowsPreview: [],
      error: str(fail.payload.error) || undefined
    };
  }
  const sourceObj = q.payload.source;
  const source =
    typeof sourceObj === "string"
      ? sourceObj
      : sourceObj && typeof sourceObj === "object"
        ? (Object.keys(sourceObj as Record<string, unknown>)[0] ?? "Llm")
        : "Llm";
  const cols = Array.isArray(q.payload.columns)
    ? (q.payload.columns as unknown[]).filter((c): c is string => typeof c === "string")
    : [];
  return {
    sql: str(q.payload.query),
    rowCount: num(q.payload.row_count),
    success: Boolean(q.payload.success),
    source,
    columns: cols,
    rowsPreview: Array.isArray(q.payload.rows) ? (q.payload.rows as unknown[][]) : [],
    error: typeof q.payload.error === "string" ? (q.payload.error as string) : undefined
  };
};

/** Extract the steps tree from the run's `subrun_started` event. The
 *  workflow orchestrator emits this once at the start of every run
 *  with the full nested DAG (`steps[].inner_tasks` recursively), so we
 *  can render structure before any per-step events arrive. Returns
 *  null when the event isn't present yet (e.g. an old run that
 *  predates `subrun_started`). */
const extractStepsTree = (events: RunEventEntry[]): SubrunStepDesc[] | null => {
  const started = events.find((e) => e.event_type === "subrun_started");
  if (!started) return null;
  const raw = started.payload.steps;
  if (!Array.isArray(raw)) return null;
  return raw as SubrunStepDesc[];
};

/** One delegation slice extracted from a container step's events:
 *  the `started/completed` pair plus the inner events that arrived as
 *  `delegation_event` envelopes in between. Each loop iteration shows
 *  up as one of these. */
interface DelegationSlice {
  childTaskId: string;
  /** Human-readable label — the orchestrator sends `"<step>[<i>]"`. */
  request: string;
  target: string;
  success: boolean;
  error?: string;
  answer?: string;
  innerEvents: RunEventEntry[];
}

/** Group every `delegation_*` event inside `events` into
 *  per-child-task slices, in start-order. */
const sliceDelegations = (events: RunEventEntry[]): DelegationSlice[] => {
  const open = new Map<
    string,
    {
      request: string;
      target: string;
      innerEvents: RunEventEntry[];
    }
  >();
  const finished: DelegationSlice[] = [];
  for (const e of events) {
    const p = e.payload as Record<string, unknown>;
    if (e.event_type === "delegation_started") {
      const id = str(p.child_task_id);
      if (!id) continue;
      open.set(id, {
        request: str(p.request),
        target: str(p.target) || "delegated",
        innerEvents: []
      });
    } else if (e.event_type === "delegation_event") {
      const id = str(p.child_task_id);
      const slot = open.get(id);
      if (!slot) continue;
      slot.innerEvents.push({
        seq: e.seq,
        event_type: str(p.inner_event_type),
        payload: (p.inner_payload as Record<string, unknown>) ?? {}
      });
    } else if (e.event_type === "delegation_completed") {
      const id = str(p.child_task_id);
      const slot = open.get(id);
      if (!slot) continue;
      open.delete(id);
      finished.push({
        childTaskId: id,
        request: slot.request,
        target: slot.target,
        success: Boolean(p.success),
        error: typeof p.error === "string" ? (p.error as string) : undefined,
        answer: typeof p.answer === "string" ? (p.answer as string) : undefined,
        innerEvents: slot.innerEvents
      });
    }
  }
  // Surface still-running iterations too so a loop mid-flight isn't
  // hidden — give them a synthetic "running" status by leaving
  // success undefined and the innerEvents we've collected so far.
  for (const [id, slot] of open.entries()) {
    finished.push({
      childTaskId: id,
      request: slot.request,
      target: slot.target,
      success: false,
      innerEvents: slot.innerEvents
    });
  }
  return finished;
};

/** Build the synthetic "iteration" node that wraps one delegation
 *  slice. Its children are the iteration's own workflow tree, built
 *  recursively from the inner events. */
const buildIterationNode = (
  slice: DelegationSlice,
  parentPath: string,
  index: number
): WorkflowNode => {
  // Each iteration is itself a workflow run — recurse with the inner
  // events as both the event source and the dag-step lookup source.
  // `subrun_step_*` events from the inner run materialise here as
  // direct events (delegation_event already unwrapped them).
  const innerSteps = innerStepsFromEvents(slice.innerEvents);
  const innerModel = buildWorkflowModel(innerSteps, slice.innerEvents);
  // Re-key the iteration's children under this parent path so React
  // keys stay unique across the whole tree.
  const reparented = innerModel.nodes.map((n, i) =>
    rekeyNode(n, `${parentPath}/iter-${index}/${i}`)
  );
  const totalMs = reparented.reduce((acc, n) => acc + (n.durationMs ?? 0), 0);
  return {
    key: `${parentPath}/iter-${index}`,
    name: slice.request || `iteration ${index + 1}`,
    status: slice.error
      ? "failed"
      : slice.success
        ? "succeeded"
        : slice.innerEvents.length === 0
          ? "pending"
          : "running",
    kind: "procedure",
    taskType: slice.target,
    durationMs: totalMs > 0 ? totalMs : null,
    startedAt: null,
    completedAt: null,
    error: slice.error,
    cached: false,
    events: slice.innerEvents,
    nestedWaterfall: undefined,
    query: undefined,
    children: reparented
  };
};

/** Re-key a node and all of its descendants. Used when grafting an
 *  inner workflow under a synthetic iteration parent. */
const rekeyNode = (node: WorkflowNode, newKey: string): WorkflowNode => ({
  ...node,
  key: newKey,
  children: node.children.map((c, i) => rekeyNode(c, `${newKey}/${i}`))
});

/** Reconstruct a flat `DagStepSummary[]` from a delegated child's
 *  inner events. We don't have the SQL view's `dag_steps` row for the
 *  child run (that lives under a different run_id), so derive the
 *  same shape locally from its `subrun_step_started/completed` pairs. */
const innerStepsFromEvents = (events: RunEventEntry[]): DagStepSummary[] => {
  const open = new Map<string, { seq: number; startedAt: string }>();
  const out: DagStepSummary[] = [];
  // We don't have real wall-clock timestamps for child events; the
  // synthetic ISO strings just preserve ordering so the model's
  // "running" / "succeeded" branching works. Duration falls back to
  // null, which the UI already handles as "…".
  for (const e of events) {
    const p = e.payload as Record<string, unknown>;
    if (e.event_type === "subrun_step_started") {
      const name = str(p.step);
      if (!name) continue;
      open.set(name, { seq: e.seq, startedAt: new Date(0).toISOString() });
    } else if (e.event_type === "subrun_step_completed") {
      const name = str(p.step);
      const slot = open.get(name);
      if (!slot) continue;
      open.delete(name);
      const success = Boolean(p.success);
      const cached = Boolean(p.cached);
      out.push({
        step_name: name,
        status: cached ? "cached" : success ? "succeeded" : "failed",
        started_at: slot.startedAt,
        completed_at: new Date(0).toISOString(),
        duration_ms: null,
        error: typeof p.error === "string" ? (p.error as string) : null,
        cached
      });
    }
  }
  // Pending starts (still running iteration steps).
  for (const [name, slot] of open.entries()) {
    out.push({
      step_name: name,
      status: "running",
      started_at: slot.startedAt,
      completed_at: null,
      duration_ms: null,
      error: null,
      cached: false
    });
  }
  return out;
};

/** Convert one `SubrunStepDesc` to a `WorkflowNode`, recursing into
 *  `inner_tasks` for container types. Runtime data (status / timing /
 *  events) comes from the `lookup` map populated from `dag_steps`. The
 *  `path` is the position-based tree key (`"2/0"` = first child of the
 *  third top-level step) — survives duplicate names in loop bodies. */
const buildNode = (
  desc: SubrunStepDesc,
  lookup: Map<string, DagStepSummary>,
  events: RunEventEntry[],
  path: string
): WorkflowNode => {
  const stepEvents = eventsForStep(events, desc.name);
  const isContainer = CONTAINER_TYPES.has(desc.task_type);

  // For containers: if there are real delegation slices in the step's
  // event window, materialize one child node per iteration (loop
  // unrolling). Otherwise fall back to the YAML's `inner_tasks`
  // template so we still show structure before any iteration fires.
  let children: WorkflowNode[];
  if (isContainer) {
    const slices = sliceDelegations(stepEvents);
    children =
      slices.length > 0
        ? slices.map((slice, i) => buildIterationNode(slice, path, i))
        : (desc.inner_tasks ?? []).map((d, i) => buildNode(d, lookup, events, `${path}/${i}`));
  } else {
    children = [];
  }

  const summary = lookup.get(desc.name);
  // Prefer the YAML's declared type over event-based detection; fall
  // back to event detection only when the type is unknown.
  const kind = kindForTaskType(desc.task_type) ?? detectKindFromEvents(stepEvents);

  // Container nodes don't get their own nested waterfall — their
  // children render the work instead.
  const nestedWaterfall =
    !isContainer && (kind === "agent" || kind === "procedure")
      ? buildWaterfall(stepEvents)
      : undefined;

  return {
    key: path,
    name: desc.name,
    status: summary?.status ?? "pending",
    kind,
    taskType: desc.task_type,
    durationMs: summary?.duration_ms ?? null,
    startedAt: summary?.started_at ?? null,
    completedAt: summary?.completed_at ?? null,
    error: summary?.error ?? undefined,
    cached: summary?.cached ?? false,
    events: stepEvents,
    nestedWaterfall,
    query: !isContainer && kind === "sql" ? extractQuery(stepEvents) : undefined,
    children
  };
};

/** Build a flat lookup from step name → runtime summary so the tree
 *  walk doesn't repeat O(n) scans. Loop iterations would collide on
 *  the same name — `dag_steps` already returns the first occurrence,
 *  matching what the tree walk binds to. */
const indexSteps = (steps: DagStepSummary[]): Map<string, DagStepSummary> => {
  const m = new Map<string, DagStepSummary>();
  for (const s of steps) m.set(s.step_name, s);
  return m;
};

/** Flatten the tree for critical-path computation — durations are
 *  per-leaf, so containers are skipped. */
const flattenLeaves = (nodes: WorkflowNode[]): WorkflowNode[] => {
  const out: WorkflowNode[] = [];
  const walk = (ns: WorkflowNode[]) => {
    for (const n of ns) {
      if (n.children.length > 0) walk(n.children);
      else out.push(n);
    }
  };
  walk(nodes);
  return out;
};

export const buildWorkflowModel = (
  steps: DagStepSummary[],
  events: RunEventEntry[]
): WorkflowModel => {
  const lookup = indexSteps(steps);
  const tree = extractStepsTree(events);

  // When `subrun_started` is present, walk its tree — this is the
  // structural path that captures `loop_sequential` / `workflow`
  // containers correctly. Otherwise fall back to a flat list from the
  // runtime summaries (matches v1 behaviour for older runs).
  const nodes: WorkflowNode[] = tree
    ? tree.map((d, i) => buildNode(d, lookup, events, String(i)))
    : steps.map((s, i) => {
        const stepEvents = eventsForStep(events, s.step_name);
        const kind = detectKindFromEvents(stepEvents);
        return {
          key: String(i),
          name: s.step_name,
          status: s.status,
          kind,
          taskType: "",
          durationMs: s.duration_ms ?? null,
          startedAt: s.started_at,
          completedAt: s.completed_at ?? null,
          error: s.error ?? undefined,
          cached: s.cached,
          events: stepEvents,
          nestedWaterfall:
            kind === "agent" || kind === "procedure" ? buildWaterfall(stepEvents) : undefined,
          query: kind === "sql" ? extractQuery(stepEvents) : undefined,
          children: []
        };
      });

  // Critical step picks the slowest leaf across the whole tree — a
  // single deep loop iteration shouldn't be hidden behind its
  // container's "duration is the sum of children" total.
  const leaves = flattenLeaves(nodes);
  let criticalLeaf: WorkflowNode | null = null;
  let maxDuration = 0;
  let totalDurationMs = 0;
  for (const leaf of leaves) {
    if (leaf.durationMs !== null) {
      totalDurationMs += leaf.durationMs;
      if (leaf.durationMs > maxDuration) {
        maxDuration = leaf.durationMs;
        criticalLeaf = leaf;
      }
    }
  }
  // `criticalIndex` historically pointed into the top-level list. With
  // a tree the index isn't a useful pointer anymore; expose the leaf
  // node directly via a new field and keep `-1` for back-compat.
  return {
    nodes,
    criticalIndex: criticalLeaf ? nodes.findIndex((n) => n.name === criticalLeaf?.name) : -1,
    criticalNode: criticalLeaf,
    totalDurationMs,
    window: deriveWindow(nodes)
  };
};

/** Walk every node in the tree and compute the earliest start +
 *  latest end across measurable rows. Used by the graph to put every
 *  bar on the same time axis so visual width corresponds to share of
 *  the run window. */
const deriveWindow = (
  nodes: WorkflowNode[]
): { t0Ms: number; t1Ms: number; spanMs: number } | null => {
  let t0 = Number.POSITIVE_INFINITY;
  let t1 = Number.NEGATIVE_INFINITY;
  const visit = (ns: WorkflowNode[]): void => {
    for (const n of ns) {
      if (n.startedAt) {
        const s = new Date(n.startedAt).getTime();
        if (Number.isFinite(s)) t0 = Math.min(t0, s);
      }
      if (n.completedAt) {
        const c = new Date(n.completedAt).getTime();
        if (Number.isFinite(c)) t1 = Math.max(t1, c);
      } else if (n.startedAt && n.durationMs) {
        const s = new Date(n.startedAt).getTime();
        if (Number.isFinite(s)) t1 = Math.max(t1, s + n.durationMs);
      }
      if (n.children.length > 0) visit(n.children);
    }
  };
  visit(nodes);
  if (!Number.isFinite(t0) || !Number.isFinite(t1) || t1 <= t0) return null;
  return { t0Ms: t0, t1Ms: t1, spanMs: t1 - t0 };
};
