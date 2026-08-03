// Metric-tree types + client. Mirrors `airlayer::engine::metric_tree*`
// over the `/<project_id>/semantic/metric-tree*` HTTP endpoints. Serde
// emits snake_case so these field names match the wire format verbatim.

import type { OxyConfig } from "./config";

// ── Tree ──────────────────────────────────────────────────────────────────────

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

// ── Sensitivity ──────────────────────────────────────────────────────────────

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

// ── Predict ──────────────────────────────────────────────────────────────────

export interface PredictChange {
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
  inputs: PredictChange[];
  impacts: PredictImpact[];
}

// ── Explain (RCA) ────────────────────────────────────────────────────────────

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
  filters: unknown[];
  delta: number;
  concentration: number;
  root_fraction: number;
  siblings?: ExplainSibling[];
  dimension_count?: number;
  children?: ExplainNode[];
}

/** Whether a driver's observed move pushes the target the way it actually
 *  moved (`contributing`) or against it (`counteracting` — it offset part of
 *  the move rather than causing it). `unknown` when no signed claim is
 *  available: `direction: unknown` with no coefficient, or a flat
 *  driver/target. */
export type DriverContribution = "contributing" | "counteracting" | "unknown";

/** A driver's move split into the part its base forced and the part its own
 *  ratio contributed. Emitted only when the driver genuinely tracks a sibling
 *  rather than moving on its own — presence is the claim.
 *  `base_driven_delta + ratio_driven_delta === driver_delta`. */
export interface PassthroughSplit {
  base_measure: string;
  ratio_previous: number;
  ratio_current: number;
  base_driven_delta: number;
  ratio_driven_delta: number;
}

export interface DriverAttribution {
  driver_measure: string;
  driver_previous: number;
  driver_current: number;
  driver_delta: number;
  /** Both optional: an `explain_cache` row written before these fields shipped
   *  is served verbatim, so absent means unclassified — not a default. */
  direction?: DriverDirection;
  contribution?: DriverContribution;
  coefficient?: number;
  form: DriverForm;
  /** Absent for a purely qualitative driver (no coefficient). */
  estimated_target_impact?: number;
  description?: string;
  passthrough?: PassthroughSplit;
}

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

// ── Opportunity ──────────────────────────────────────────────────────────────

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

export interface OpportunityRequest {
  target: string;
  time_dimension: string;
  period: [string, string];
}

export interface OpportunityResult {
  target: string;
  period: [string, string];
  overall_value: number;
  /**
   * "rows" (rate-based additive sizing — the only basis that yields a sized
   * upside figure), "value_share" (additive) or "equal" (ratios).
   */
  weight_basis: string;
  dimensions: DimensionOpportunity[];
  skipped_dimensions: SkippedDimension[];
  downstream: PredictImpact[];
}

// ── Distribution ─────────────────────────────────────────────────────────────

/**
 * Single-period structural decomposition. The server auto-derives the
 * baseline as the equal-length window immediately before `period`, then
 * returns an {@link ExplainResult}-shaped payload (so the same renderers
 * work). Ignore the delta fields when rendering a pure distribution.
 */
export interface DistributionRequest {
  target: string;
  time_dimension: string;
  /** `[start, end]` inclusive date strings. */
  period: [string, string];
}

// ── Time dimensions ──────────────────────────────────────────────────────────

export interface TimeDimensionsResponse {
  /** view name → fully-qualified time-dimension ids (`view.dim`). */
  by_view: Record<string, string[]>;
}

// ── Client ───────────────────────────────────────────────────────────────────

/**
 * Shape of the inner request helper exposed by `OxyClient`. The metric-tree
 * client reuses it to inherit auth headers, timeout, baseUrl, and project
 * scoping rather than reimplementing fetch end-to-end.
 */
export type RequestFn = <T>(endpoint: string, options?: RequestInit) => Promise<T>;

/**
 * Client for the `/semantic/metric-tree*` endpoints. Surfaces the four
 * airlayer metric-tree analyses (tree introspection, sensitivity, predict,
 * explain, opportunity) over typed methods.
 *
 * Construction is internal to {@link OxyClient} — call `client.metricTree`
 * to access an instance rather than building one yourself.
 *
 * @example
 * ```typescript
 * const client = await OxyClient.create({ projectId: "...", apiKey: "..." });
 * const tree = await client.metricTree.getTree();
 * const drivers = await client.metricTree.getSensitivity("orders.net_revenue");
 * ```
 */
export class MetricTreeClient {
  private readonly request: RequestFn;
  private readonly config: OxyConfig;

  constructor(config: OxyConfig, request: RequestFn) {
    this.config = config;
    this.request = request;
  }

  private path(suffix: string): string {
    return `/${this.config.projectId}/semantic/metric-tree${suffix}`;
  }

  private buildQuery(extra: Record<string, string> = {}): string {
    const params: Record<string, string> = { ...extra };
    if (this.config.branch) params.branch = this.config.branch;
    const qs = new URLSearchParams(params).toString();
    return qs ? `?${qs}` : "";
  }

  /**
   * Fetch the full metric tree, or the subtree rooted at `root`.
   *
   * @param root - Optional fully-qualified measure id to root the tree at.
   * @returns Nodes (measures) and edges (component / driver relationships).
   *
   * @example
   * ```typescript
   * const tree = await client.metricTree.getTree();
   * const subtree = await client.metricTree.getTree("orders.net_revenue");
   * ```
   */
  async getTree(root?: string): Promise<MetricTree> {
    const query = this.buildQuery(root ? { root } : {});
    return this.request<MetricTree>(this.path(query));
  }

  /**
   * Rank the declared drivers of a measure by influence.
   *
   * @param measureId - Fully-qualified measure id (`view.measure`).
   *
   * @example
   * ```typescript
   * const sensitivity = await client.metricTree.getSensitivity("orders.net_revenue");
   * for (const driver of sensitivity.drivers) {
   *   console.log(driver.measure, driver.direction, driver.strength);
   * }
   * ```
   */
  async getSensitivity(measureId: string): Promise<SensitivityResult> {
    const query = this.buildQuery();
    return this.request<SensitivityResult>(
      this.path(`/${encodeURIComponent(measureId)}/sensitivity${query}`)
    );
  }

  /**
   * Propagate hypothetical `(measure, delta)` changes upward through the
   * tree. Returns the estimated impact on every downstream measure.
   *
   * @example
   * ```typescript
   * const result = await client.metricTree.predict([
   *   { measure: "marketing_spend.total_spend", delta: 10000 },
   * ]);
   * ```
   */
  async predict(changes: PredictChange[]): Promise<PredictResult> {
    const query = this.buildQuery();
    return this.request<PredictResult>(this.path(`/predict${query}`), {
      method: "POST",
      body: JSON.stringify({ changes })
    });
  }

  /**
   * Period-over-period root-cause decomposition. Recursively splits the
   * target measure by components and dimensions until the move concentrates.
   *
   * @example
   * ```typescript
   * const result = await client.metricTree.explain({
   *   target: "financials.operating_profit",
   *   time_dimension: "financials.month",
   *   current_period: ["2025-09-01", "2025-09-30"],
   *   previous_period: ["2025-08-01", "2025-08-31"],
   * });
   * ```
   */
  async explain(request: ExplainRequest): Promise<ExplainResult> {
    const query = this.buildQuery();
    return this.request<ExplainResult>(this.path(`/explain${query}`), {
      method: "POST",
      body: JSON.stringify(request)
    });
  }

  /**
   * Size the upside opportunity for a measure by finding underperforming
   * segments. Skips high-cardinality dimensions and trims the long tail.
   *
   * @example
   * ```typescript
   * const result = await client.metricTree.findOpportunities({
   *   target: "orders.net_revenue",
   *   time_dimension: "orders.order_date",
   *   period: ["2025-09-01", "2025-09-30"],
   * });
   * for (const dim of result.dimensions) {
   *   console.log(dim.dimension, "+", dim.total_upside);
   * }
   * ```
   */
  async findOpportunities(request: OpportunityRequest): Promise<OpportunityResult> {
    const query = this.buildQuery();
    return this.request<OpportunityResult>(this.path(`/opportunity${query}`), {
      method: "POST",
      body: JSON.stringify(request)
    });
  }
}
