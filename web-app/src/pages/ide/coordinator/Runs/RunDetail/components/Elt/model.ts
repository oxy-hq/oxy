import type { EltTableSummary, RunEventEntry } from "@/services/api/coordinator";

/**
 * ELT (airway) run model. Combines the backend's `elt_tables` summary
 * (rows + extract/load timestamps reconstructed from the events table)
 * with the rich per-phase event log so the UI can render the
 * extract → normalize → load progression that the airway engine
 * actually emits.
 *
 * Phase windows come from paired `<phase>_started` / `<phase>_completed`
 * events per table. Row counts come from the completion payloads
 * (`rows_extracted`, `rows_normalized`, `rows`). Normalize is
 * optional — connectors that don't relationally normalize skip those
 * events entirely.
 */
export interface EltPipelineModel {
  /** From the run's `pipeline_plan` event. `null` for legacy airway
   *  runs that predate the event. */
  pipelineName: string | null;
  /** Source connector / pipeline name surfaced as the lineage left card. */
  sourceLabel: string;
  /** Destination connector surfaced as the lineage right card. */
  destination: string | null;
  /** List of declared tables from the plan, in declaration order.
   *  Used as a stable order for the per-table cards even before the
   *  first extract event arrives. */
  resources: string[];
  tables: EltTableNode[];
  /** Schema-evolution events emitted during the run — surfaced as a
   *  top banner so operators notice column additions immediately. */
  schemaChanges: SchemaChange[];
  /** Aggregate row counts across every table. */
  rollup: {
    rowsExtracted: number;
    rowsLoaded: number;
    totalTables: number;
    failedTables: number;
    loadedTables: number;
  };
  /** Pipeline-level fatal error (`pipeline_error` payload), if any. */
  pipelineError: string | null;
  /** True when the run carried a `cancelled` event. */
  cancelled: boolean;
  /** Shared time window for all phase bars. `null` when no phase has
   *  fired yet. */
  window: { t0Ms: number; t1Ms: number; spanMs: number } | null;
}

export interface EltTableNode {
  /** Logical table name as it appears in the airway plan / events. */
  name: string;
  /** "loaded" | "failed" | "loading" | "normalizing" | "extracting" |
   *  "extracted" | "pending" — derived from the most-advanced phase
   *  marker seen for this table. */
  status: EltTableStatus;
  /** Extract phase window. `null` when extract hasn't started. */
  extract: PhaseSpan | null;
  /** Normalize phase window. `null` when normalize hasn't started (or
   *  the connector doesn't normalize). */
  normalize: PhaseSpan | null;
  /** Load phase window — destination write. */
  load: PhaseSpan | null;
  /** Rows pulled from the source connector. */
  rowsExtracted: number | null;
  /** Rows that landed on the destination after normalize. */
  rowsLoaded: number | null;
  /** Rows after normalize but before destination write. */
  rowsNormalized: number | null;
  /** Child tables produced by relational normalization (rare). */
  childTables: string[];
  /** Drop% from extract → load, rounded; `null` when not computable. */
  dropPct: number | null;
  /** Per-table failure message from `table_load_failed` or
   *  `resource_failed`. */
  error: string | null;
}

export type EltTableStatus =
  | "loaded"
  | "failed"
  | "loading"
  | "normalizing"
  | "extracting"
  | "extracted"
  | "pending";

export interface PhaseSpan {
  /** Absolute ms of when this phase started. */
  startedAtMs: number;
  /** Absolute ms of when this phase ended. `null` while still running. */
  completedAtMs: number | null;
}

export interface SchemaChange {
  seq: number;
  pipelineName: string;
  /** Raw JSON payload from the airway `schema_evolved` event — the
   *  airway engine's `SchemaChange` shape, exposed as JSON so the
   *  serde contract doesn't drift with airway internals. */
  changes: unknown;
}

const num = (v: unknown): number => (typeof v === "number" ? v : 0);
const str = (v: unknown): string => (typeof v === "string" ? v : "");
const stringArray = (v: unknown): string[] =>
  Array.isArray(v) ? v.filter((x): x is string => typeof x === "string") : [];

/** Index into open phase windows by `(phase, table)`. Closed when the
 *  matching `*_completed` arrives. */
type Phase = "extract" | "normalize" | "load";

const eventTimeMs = (e: RunEventEntry): number | null => {
  // The backend doesn't currently persist a per-event timestamp on the
  // row payload, so derive ordering from seq. Sequence numbers are
  // monotonic per-run; treating them as a logical clock (~1 unit per
  // event) gives stable bar positions even when wall-clock data is
  // missing.
  if (typeof e.seq === "number") return e.seq;
  return null;
};

/** Lineage labels + run-level error stamped on the run row. Optional —
 *  callers without them fall back to `pipeline_plan` event payload and
 *  the in-event `pipeline_error` respectively. */
export interface EltMetadata {
  pipelineName?: string | null;
  sourceKind?: string | null;
  destinationLabel?: string | null;
  /** Run-row `error_message` — set when the run failed *before* the
   *  airway worker emitted any events (e.g. secret resolution, spec
   *  validation). The in-event `pipeline_error` payload is preferred
   *  when both are present, since it carries more detail. */
  runError?: string | null;
}

export const buildEltModel = (
  tables: EltTableSummary[],
  events: RunEventEntry[],
  metadata: EltMetadata = {}
): EltPipelineModel => {
  // Pipeline plan — first event in a healthy run. Carries the source
  // pipeline name + the list of declared resources + destination.
  // Falls back to the run-row metadata stamped at start time so the
  // lineage cards have real names even before the event fires (or
  // for older runs that predate it).
  const plan = events.find((e) => e.event_type === "pipeline_plan");
  const planPipelineName = plan ? str(plan.payload.pipeline_name) : "";
  const planDestination = plan ? str(plan.payload.destination) : "";
  const pipelineName = planPipelineName || metadata.pipelineName || null;
  const destination = planDestination || metadata.destinationLabel || null;
  const declaredResources = plan ? stringArray(plan.payload.resources) : [];

  // Walk events to populate per-table phase windows + row counts +
  // schema changes + pipeline-level errors.
  const tableState = new Map<
    string,
    {
      extract: PhaseSpan | null;
      normalize: PhaseSpan | null;
      load: PhaseSpan | null;
      rowsExtracted: number | null;
      rowsLoaded: number | null;
      rowsNormalized: number | null;
      childTables: string[];
      error: string | null;
      currentPhase: Phase | null;
    }
  >();

  const ensure = (name: string) => {
    let s = tableState.get(name);
    if (!s) {
      s = {
        extract: null,
        normalize: null,
        load: null,
        rowsExtracted: null,
        rowsLoaded: null,
        rowsNormalized: null,
        childTables: [],
        error: null,
        currentPhase: null
      };
      tableState.set(name, s);
    }
    return s;
  };

  const openPhase = (name: string, phase: Phase, atMs: number) => {
    const s = ensure(name);
    s[phase] = { startedAtMs: atMs, completedAtMs: null };
    s.currentPhase = phase;
  };

  const closePhase = (name: string, phase: Phase, atMs: number) => {
    const s = ensure(name);
    const open = s[phase];
    if (open) s[phase] = { ...open, completedAtMs: atMs };
    if (s.currentPhase === phase) s.currentPhase = null;
  };

  const schemaChanges: SchemaChange[] = [];
  // Seed from the run row's `error_message` so pre-flight failures
  // (secret resolution, spec validation — they happen before the
  // worker starts and therefore never emit a `pipeline_error` event)
  // still surface on the page. An in-event `pipeline_error` later in
  // the loop overwrites this with the richer payload when present.
  let pipelineError: string | null = metadata.runError ?? null;
  let cancelled = false;

  for (const e of events) {
    const p = e.payload as Record<string, unknown>;
    const at = eventTimeMs(e) ?? 0;
    switch (e.event_type) {
      case "extract_started":
        openPhase(str(p.table), "extract", at);
        break;
      case "extract_completed": {
        const t = str(p.table);
        closePhase(t, "extract", at);
        const s = ensure(t);
        s.rowsExtracted = num(p.rows_extracted);
        break;
      }
      case "normalize_started":
        openPhase(str(p.table), "normalize", at);
        break;
      case "normalize_completed": {
        const t = str(p.table);
        closePhase(t, "normalize", at);
        const s = ensure(t);
        s.rowsNormalized = num(p.rows_normalized);
        s.childTables = stringArray(p.child_tables);
        break;
      }
      case "table_load_started":
        openPhase(str(p.table), "load", at);
        break;
      case "table_loaded": {
        const t = str(p.table);
        closePhase(t, "load", at);
        const s = ensure(t);
        s.rowsLoaded = num(p.rows);
        break;
      }
      case "table_load_failed": {
        const t = str(p.table);
        closePhase(t, "load", at);
        const s = ensure(t);
        s.error = str(p.error) || "load failed";
        break;
      }
      case "resource_failed": {
        const t = str(p.table);
        const s = ensure(t);
        s.error = str(p.error) || "extract failed";
        s.currentPhase = null;
        break;
      }
      case "schema_evolved":
        schemaChanges.push({
          seq: e.seq,
          pipelineName: str(p.pipeline_name),
          changes: p.changes ?? null
        });
        break;
      case "pipeline_error":
        pipelineError = str(p.error);
        break;
      case "cancelled":
        cancelled = true;
        break;
      default:
        break;
    }
  }

  // Merge in the backend's `elt_tables` summary as a fallback —
  // wall-clock timestamps live there even though we use seq-derived
  // positions for the bar axis (consistent across all tables).
  const summaryByName = new Map<string, EltTableSummary>();
  for (const t of tables) summaryByName.set(t.table_name, t);

  // Final table list: union of (declared plan resources, summary
  // names, event names) so a table that's only been declared but
  // hasn't started still appears as a pending row.
  const allNames = new Set<string>();
  for (const r of declaredResources) allNames.add(r);
  for (const t of tables) allNames.add(t.table_name);
  for (const name of tableState.keys()) allNames.add(name);

  const tableNodes: EltTableNode[] = Array.from(allNames).map((name) => {
    const s = tableState.get(name);
    const summary = summaryByName.get(name);
    const rowsExtracted = s?.rowsExtracted ?? summary?.rows_extracted ?? null;
    const rowsLoaded = s?.rowsLoaded ?? summary?.rows_loaded ?? null;
    const dropPct =
      rowsExtracted !== null &&
      rowsLoaded !== null &&
      rowsExtracted > 0 &&
      rowsLoaded < rowsExtracted
        ? Math.round(((rowsExtracted - rowsLoaded) / rowsExtracted) * 100)
        : null;
    return {
      name,
      status: deriveStatus(s, summary),
      extract: s?.extract ?? null,
      normalize: s?.normalize ?? null,
      load: s?.load ?? null,
      rowsExtracted,
      rowsLoaded,
      rowsNormalized: s?.rowsNormalized ?? null,
      childTables: s?.childTables ?? [],
      dropPct,
      error: s?.error ?? null
    };
  });

  // Rollups for the lineage header.
  const rollup = {
    rowsExtracted: tableNodes.reduce((a, t) => a + (t.rowsExtracted ?? 0), 0),
    rowsLoaded: tableNodes.reduce((a, t) => a + (t.rowsLoaded ?? 0), 0),
    totalTables: tableNodes.length,
    failedTables: tableNodes.filter((t) => t.status === "failed").length,
    loadedTables: tableNodes.filter((t) => t.status === "loaded").length
  };

  // Shared time axis across every phase window in every table.
  let t0 = Number.POSITIVE_INFINITY;
  let t1 = Number.NEGATIVE_INFINITY;
  for (const t of tableNodes) {
    for (const phase of [t.extract, t.normalize, t.load]) {
      if (!phase) continue;
      t0 = Math.min(t0, phase.startedAtMs);
      t1 = Math.max(t1, phase.completedAtMs ?? phase.startedAtMs);
    }
  }
  const window =
    Number.isFinite(t0) && Number.isFinite(t1) && t1 > t0
      ? { t0Ms: t0, t1Ms: t1, spanMs: t1 - t0 }
      : null;

  // Source label preference: connector kind from metadata
  // (`postgres_cdc` / `stripe` / `rest_api` — the most informative
  // label, comes from the YAML's `source.kind`) → pipeline name → the
  // generic fallback. Pipeline name is human-given (e.g.
  // `"stripe_to_bq"`) and almost always more readable than the
  // generic placeholder.
  const sourceLabel = metadata.sourceKind || pipelineName || "source";

  return {
    pipelineName,
    sourceLabel,
    destination,
    resources: declaredResources,
    tables: tableNodes,
    schemaChanges,
    rollup,
    pipelineError,
    cancelled,
    window
  };
};

const deriveStatus = (
  s:
    | {
        extract: PhaseSpan | null;
        normalize: PhaseSpan | null;
        load: PhaseSpan | null;
        error: string | null;
        currentPhase: Phase | null;
      }
    | undefined,
  summary: EltTableSummary | undefined
): EltTableStatus => {
  if (s?.error) return "failed";
  if (s?.load?.completedAtMs !== null && s?.load) return "loaded";
  if (s?.load) return "loading";
  if (s?.normalize?.completedAtMs !== null && s?.normalize) return "normalizing";
  if (s?.normalize) return "normalizing";
  if (s?.extract?.completedAtMs !== null && s?.extract) return "extracted";
  if (s?.extract) return "extracting";
  // Fall back to summary status if events haven't been observed yet.
  if (summary) {
    const m = summary.status;
    if (m === "loaded") return "loaded";
    if (m === "failed") return "failed";
    if (m === "loading") return "loading";
    if (m === "extracting") return "extracting";
    if (m === "extracted") return "extracted";
  }
  return "pending";
};
