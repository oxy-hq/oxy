// Metric tree types — mirror the airlayer structs serialized by the
// `/semantic/metric-tree*` endpoints (snake_case, as serde emits them).

export type EdgeKind = "component" | "driver";
export type DriverDirection = "positive" | "negative" | "unknown";
export type DriverStrength = "strong" | "moderate" | "weak";
export type DriverConfidence = "high" | "medium" | "low";
export type DriverForm = "linear" | "log-log" | "log-linear" | "linear-log";

export interface MetricNode {
  id: string;
  view: string;
  measure: string;
  label: string;
  description?: string | null;
  measure_type: string;
  is_composite: boolean;
  expr?: string | null;
  /** Whether the drill will size this measure on a per-unit rate — airlayer's
   *  `supports_rate_basis(layer, target)`, computed once at tree build and
   *  serialized here so consumers don't re-derive eligibility from
   *  `measure_type` or component-edge presence (both get it wrong: a `type:
   *  sum` check misses eligible composites, and edge presence over-admits
   *  nested/cross-view/multiplicative passthroughs the engine refuses). */
  drillable: boolean;
}

export interface MetricEdge {
  from: string;
  to: string;
  kind: EdgeKind;
  /** Sign of a component edge; omitted (defaults to +1) for most edges. */
  sign?: number;
  direction: DriverDirection;
  strength: DriverStrength;
  confidence: DriverConfidence;
  coefficient?: number | null;
  form: DriverForm;
  intercept?: number | null;
  lag?: number | null;
  description?: string | null;
  refs?: string[] | null;
}

export interface MetricTree {
  nodes: MetricNode[];
  edges: MetricEdge[];
  root?: string | null;
}

// ── sensitivity ─────────────────────────────────────────────────────────────

export interface SensitivityDriver {
  measure: string;
  path: string[];
  edge_kind: string;
  effective_coefficient?: number | null;
  form?: DriverForm | null;
  direction: DriverDirection;
  strength: DriverStrength;
  lag?: number | null;
  description?: string | null;
}

export interface SensitivityResult {
  target: string;
  drivers: SensitivityDriver[];
}

// ── predict ─────────────────────────────────────────────────────────────────

export interface PredictInput {
  measure: string;
  delta: number;
}

export interface PredictImpact {
  measure: string;
  estimated_delta: number;
  confidence: string;
  path: string[];
  form: DriverForm;
  lag?: number | null;
}

export interface PredictResult {
  inputs: PredictInput[];
  impacts: PredictImpact[];
}

// ── explain ─────────────────────────────────────────────────────────────────

export type SplitKind =
  | { type: "component"; child_measure: string }
  | { type: "dimension"; dimension: string; value: string }
  | { type: "uniform_degradation"; dimension: string; num_elements: number }
  | { type: "cross_cutting"; dimension: string; value: string; measures: string[] };

export interface ExplainSibling {
  split: SplitKind;
  measure: string;
  delta: number;
  root_fraction: number;
}

export interface ExplainNode {
  split: SplitKind;
  measure: string;
  /** Accumulated dimension-split filters; opaque to the UI. */
  filters: unknown[];
  delta: number;
  concentration: number;
  root_fraction: number;
  siblings?: ExplainSibling[];
  dimension_count?: number;
  children?: ExplainNode[];
}

export interface DriverAttribution {
  driver_measure: string;
  driver_previous: number;
  driver_current: number;
  driver_delta: number;
  coefficient?: number;
  form: DriverForm;
  estimated_target_impact?: number;
  description?: string;
}

/** Mirrors `airlayer::engine::metric_tree_ops::ExplainWarning` —
 *  internally-tagged enum, `type` selects the variant. */
export type ExplainWarning =
  | {
      type: "simpsons_paradox";
      dimension: string;
      aggregate_delta: number;
      segment_directions: [string, number][];
    }
  | {
      type: "opposing_offset";
      component_a: string;
      component_b: string;
      delta_a: number;
      delta_b: number;
    }
  | {
      type: "non_additive_dimension_split";
      measure: string;
      measure_type: string;
      dimension: string;
    };

export interface ExplainResult {
  target: string;
  target_delta: number;
  target_previous: number;
  target_current: number;
  time_dimension: string;
  current_period: [string, string];
  previous_period: [string, string];
  nodes: ExplainNode[];
  coverage: number;
  driver_attribution?: DriverAttribution[];
  alternatives?: unknown[];
  warnings?: ExplainWarning[];
}

// ── opportunity ─────────────────────────────────────────────────────────────

export interface SegmentOpportunity {
  segment: string;
  /** Benchmarked figure: a per-unit RATE (value / rows) when `weight_basis` is
   *  "rows", otherwise the raw measure value. */
  current_value: number;
  /** Row count in "rows" mode, a value share otherwise. */
  volume: number;
  /** Benchmark being compared against — a rate in "rows" mode. */
  benchmark: number;
  /** Gap to benchmark, same units as `current_value`. */
  gap: number;
  /** Addressable upside in measure units. In "rows" mode this is the rate gap
   *  applied to the segment's own volume, `(benchmark_rate − rate) × count`. */
  upside: number;
}

export interface DimensionOpportunity {
  dimension: string;
  cardinality: number;
  /** "best_peer" or "p75". */
  benchmark_basis: string;
  /** Summed over the significant segments, and summed BEFORE the tail trim and
   *  top-K cut — so it can exceed the sum of `segments`. */
  total_upside: number;
  segments: SegmentOpportunity[];
  /** Real segments left out of `segments`: those under 1% of the dimension's
   *  upside, plus any past the top-5 cap. The latter need not be small. */
  other_segments_skipped: number;
  /** Segments below the benchmark whose gap could not be distinguished from
   *  sampling noise, so were not sized. Distinct from the above: those were
   *  omitted for being minor, these for being unproven. */
  segments_dropped_as_noise: number;
}

export interface SkippedDimension {
  dimension: string;
  reason: string;
}

export interface OpportunityResult {
  target: string;
  period: [string, string];
  overall_value: number;
  /** How segments were weighted/compared: "rows" (sum-like sized on a per-unit
   *  rate with a `count` denominator), "value_share" (avg/min/max), or "equal"
   *  (ratios). Drives how to read `current_value`/`benchmark`/`gap`/`upside`. */
  weight_basis: string;
  dimensions: DimensionOpportunity[];
  skipped_dimensions: SkippedDimension[];
  downstream: PredictImpact[];
  /** `view.measure` id of the `count` measure the target was divided by to form
   *  the per-unit rates in `current_value`/`benchmark`. Present only in "rows"
   *  mode — the only mode that forms rates. Added by Oxy's handler, not
   *  airlayer: without it a rate is an unlabelled number. */
  rate_denominator?: string | null;
}

// ── drill ───────────────────────────────────────────────────────────────────

/** Mirrors `airlayer::engine::metric_tree_ops::CandidateKind` — externally
 *  tagged (no serde attrs): the variant name is the sole object key. */
export type CandidateKind =
  | { Component: { measure: string } }
  | { Dimension: { dimension: string; value: string } };

/** Mirrors `airlayer::engine::metric_tree_ops::StopReason` — a unit-variant
 *  enum, so serde emits it as a bare string. */
export type StopReason = "GateFailed" | "GateInconclusive" | "NoCandidates" | "MaxDepth";

/** A minimal mirror of airlayer's `QueryFilter` — the panel only needs
 *  `member` + `values` off of it. */
export interface DrillFilter {
  member?: string;
  values: string[];
}

export interface DrillCandidate {
  kind: CandidateKind;
  /** Share of THIS level's gap. */
  concentration: number;
  gap: number;
  gated: boolean;
}

export interface DrillLevel {
  measure: string;
  /** Accumulated numerator filters down to this level. */
  segment_filter: DrillFilter[];
  gap: number;
  /** Cascaded fraction of the ROOT gap. */
  root_share: number;
  /** Ranked; `[0]` is the one recursed into unless `stop_reason` is set. */
  candidates: DrillCandidate[];
  stop_reason: StopReason | null;
}

export interface DrillResult {
  target: string;
  root_gap: number;
  root_upside: number;
  benchmark_filter: DrillFilter[];
  levels: DrillLevel[];
}

/** Response of `POST /semantic/metric-tree/drill`. Mirrors
 *  `DrillResponse { #[serde(flatten)] result: Option<DrillResult>, rate_denominator }`:
 *  a `Some` result flattens `DrillResult`'s fields to the top level; a `None`
 *  omits them entirely (no `levels`). Model all `DrillResult` fields as
 *  optional here and treat a missing `levels` as "nothing to drill." */
export interface DrillResponse {
  target?: string;
  root_gap?: number;
  root_upside?: number;
  benchmark_filter?: DrillFilter[];
  levels?: DrillLevel[];
  /** Same role as `OpportunityResult.rate_denominator`: the `count` measure
   *  id the target was divided by to form rates, when applicable. */
  rate_denominator?: string | null;
}

/** Mirrors `airlayer::engine::metric_tree_ops::DrillRoot`. Names WHICH ranked
 *  row to decompose; the engine still derives that row's benchmark, gap and
 *  upside from its own scan. */
export interface DrillRoot {
  dimension: string;
  segment: string;
}

/** Request payload for `POST /semantic/metric-tree/drill`. */
export interface DrillRequest {
  target: string;
  time_dimension: string;
  period: [string, string];
  /** Narrow the scan to one instance. Omit to size the whole population. */
  instance?: OpportunityInstance;
  /** Optional override; defaults to airlayer's `DrillConfig::default()` (max_depth 5). */
  max_depth?: number;
  /** Optional override of the single-scan significance budget. */
  alpha?: number;
  /** Decompose this ranked row instead of the engine's top pick. Omit for the
   *  top pick. A row that is no longer in the scan comes back with no `levels`. */
  root?: DrillRoot;
}

// ── request payloads ────────────────────────────────────────────────────────

export interface PredictChange {
  measure: string;
  delta: number;
}

export interface ExplainConfigOverride {
  deep?: boolean;
  max_depth?: number;
  coverage_threshold?: number;
}

export interface ExplainRequest {
  target: string;
  time_dimension: string;
  current_period: [string, string];
  previous_period: [string, string];
  config?: ExplainConfigOverride;
}

/** A world-model instance to scope a scan to, addressed as the world-model API
 *  addresses one: entity name plus instance key (a JSON array for a composite
 *  key, else a bare scalar). */
export interface OpportunityInstance {
  entity: string;
  key: string;
}

export interface OpportunityRequest {
  target: string;
  time_dimension: string;
  period: [string, string];
  /** Narrow the scan to one instance. Omit to size the whole population. */
  instance?: OpportunityInstance;
}

/** Response of `GET /semantic/metric-tree/time-dimensions`. */
export interface TimeDimensionsResponse {
  /** view name → fully-qualified time-dim ids (`view.dim`). */
  by_view: Record<string, string[]>;
}

/** Single-period distribution request. The baseline is auto-derived
 *  server-side as the equal-length window immediately before `period`. */
export interface DistributionRequest {
  target: string;
  time_dimension: string;
  period: [string, string];
}
