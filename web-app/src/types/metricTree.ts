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
  current_value: number;
  volume: number;
  benchmark: number;
  gap: number;
  /** Match-the-best upside in measure units. */
  upside: number;
}

export interface DimensionOpportunity {
  dimension: string;
  cardinality: number;
  /** "best_peer" or "p75". */
  benchmark_basis: string;
  total_upside: number;
  segments: SegmentOpportunity[];
  other_segments_skipped: number;
}

export interface SkippedDimension {
  dimension: string;
  reason: string;
}

export interface OpportunityResult {
  target: string;
  period: [string, string];
  overall_value: number;
  /** "value_share" (additive) or "equal" (ratios). */
  weight_basis: string;
  dimensions: DimensionOpportunity[];
  skipped_dimensions: SkippedDimension[];
  downstream: PredictImpact[];
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

export interface OpportunityRequest {
  target: string;
  time_dimension: string;
  period: [string, string];
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
