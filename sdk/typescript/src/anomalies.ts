// Anomaly inbox types + client. Surfaces the `/semantic/anomalies*`
// endpoints — list, scan, status, explain — so SDK consumers can render
// the same inbox the Oxy IDE uses.

import type { OxyConfig } from "./config";
import type { ExplainResult } from "./metricTree";

// ── Types ────────────────────────────────────────────────────────────────────

export type AnomalyStatus = "new" | "acknowledged" | "dismissed";
export type AnomalySeverity = "low" | "medium" | "high";

/** One filter pinning an anomaly (or a failed monitor) to a segment. */
export interface AnomalyFilter {
  /** Fully-qualified dimension id, e.g. `"sales_daily.restaurant_id"`. */
  member: string;
  /** Matched values (OR within a filter). */
  values: string[];
}

/**
 * One row in the anomaly inbox. Detected by `oxy-metric-monitoring` per
 * `.monitor.yml` entry; upserted by repeat scans so unresolved anomalies
 * stay visible without piling up duplicates.
 */
export interface Anomaly {
  id: string;
  workspace_id: string;
  measure: string;
  time_dimension: string;
  granularity: string;
  period_start: string;
  period_end: string;
  observed: number;
  expected: number;
  lower_bound: number;
  upper_bound: number;
  z_score: number;
  severity: AnomalySeverity | string;
  status: AnomalyStatus | string;
  label?: string | null;
  /**
   * Stable key derived from the monitor's filters (e.g.
   * `"sales_daily.restaurant_id=loc-abc"`). Empty for chain-wide monitors.
   */
  dimension_key: string;
  /**
   * Raw filters identifying the segment; `null` for chain-wide monitors.
   * Always present on the wire (the server serializes it unconditionally),
   * hence required-nullable rather than optional — same shape as
   * {@link ScanFailure.filters}.
   */
  filters: AnomalyFilter[] | null;
  /** Cached ExplainResult — populated by `POST /anomalies/:id/explain`. */
  explain_cache?: ExplainResult | null;
  explain_cached_at?: string | null;
  detected_at: string;
  updated_at: string;
}

export interface ListAnomaliesOptions {
  status?: AnomalyStatus | string;
  /** Max rows (server caps at 500, defaults to 100). */
  limit?: number;
}

export interface ListAnomaliesResponse {
  anomalies: Anomaly[];
}

export interface ScanOptions {
  /** Override the reference "now" date (YYYY-MM-DD) — useful for demos. */
  as_of?: string;
}

/** One `.monitor.yml` entry that errored during a scan. */
export interface ScanFailure {
  measure: string;
  time_dimension: string;
  granularity: string;
  label: string | null;
  /** Segment key for a `group_by`/filtered monitor; empty for chain-wide. */
  dimension_key: string;
  /** Raw filters identifying the segment; null for chain-wide monitors. */
  filters: AnomalyFilter[] | null;
  error: string;
}

export interface ScanResponse {
  monitors_scanned: number;
  monitors_failed: number;
  anomalies_persisted: number;
  /**
   * True when the scan is still running server-side (it exceeded the 55 s
   * synchronous window, or a scan started within the last 60 s and this call
   * was debounced). The counts are all `0` in that case — they are NOT a
   * "nothing found" result. Refetch with `list()` after a short delay.
   */
  pending: boolean;
  /**
   * Per-monitor failures. Empty array (never absent) on a clean scan and on
   * the `pending` path, where failures aren't known yet.
   */
  failures: ScanFailure[];
}

export interface ExplainOptions {
  /** Recompute even when the row already has a cached result. */
  refresh?: boolean;
}

// ── Client ───────────────────────────────────────────────────────────────────

export type RequestFn = <T>(endpoint: string, options?: RequestInit) => Promise<T>;

/**
 * Client for `/semantic/anomalies*`. Construct via `OxyClient.anomalies`
 * rather than instantiating directly — the getter wires the request helper
 * so auth, timeout, and branch propagation come along for free.
 *
 * @example
 * ```typescript
 * const { anomalies } = await client.anomalies.list({ status: "new" });
 * for (const a of anomalies) {
 *   console.log(a.label ?? a.measure, a.severity, a.z_score.toFixed(2));
 * }
 * ```
 */
export class AnomaliesClient {
  private readonly request: RequestFn;
  private readonly config: OxyConfig;

  constructor(config: OxyConfig, request: RequestFn) {
    this.config = config;
    this.request = request;
  }

  private path(suffix: string): string {
    return `/${this.config.projectId}/semantic/anomalies${suffix}`;
  }

  private buildQuery(extra: Record<string, string> = {}): string {
    const params: Record<string, string> = { ...extra };
    if (this.config.branch) params.branch = this.config.branch;
    const qs = new URLSearchParams(params).toString();
    return qs ? `?${qs}` : "";
  }

  /**
   * List anomalies in the inbox, newest first.
   *
   * @example
   * ```typescript
   * // Open / unresolved anomalies only
   * const { anomalies } = await client.anomalies.list({ status: "new" });
   * ```
   */
  async list(options: ListAnomaliesOptions = {}): Promise<ListAnomaliesResponse> {
    const extra: Record<string, string> = {};
    if (options.status) extra.status = options.status;
    if (options.limit) extra.limit = String(options.limit);
    // No trailing slash before the query — axum 307-redirects "/anomalies/"
    // to "/anomalies", and the redirect fails CORS preflight in browsers.
    return this.request<ListAnomaliesResponse>(this.path(this.buildQuery(extra)));
  }

  /**
   * Trigger a full scan. Iterates every `.monitor.yml` entry in the
   * workspace, runs the detector, and upserts matching rows into the
   * inbox. Returns counts of scanned / failed / persisted.
   *
   * Long-running: the server waits up to 55 s, then returns
   * `pending: true` with zeroed counts while the scan finishes in the
   * background. Always check `pending` before treating `0` as "nothing
   * found", and refetch with {@link list} shortly after.
   *
   * @example
   * ```typescript
   * // Scan against a known-good reference date (matches the seed dataset)
   * const result = await client.anomalies.scan({ as_of: "2025-12-15" });
   * if (result.pending) {
   *   console.log("scan still running — refetch shortly");
   * } else {
   *   console.log(`${result.anomalies_persisted} anomalies detected`);
   * }
   * ```
   */
  async scan(options: ScanOptions = {}): Promise<ScanResponse> {
    const extra: Record<string, string> = {};
    if (options.as_of) extra.as_of = options.as_of;
    return this.request<ScanResponse>(this.path(`/scan${this.buildQuery(extra)}`), {
      method: "POST"
    });
  }

  /**
   * Update an anomaly's status (acknowledge / dismiss / re-open).
   */
  async updateStatus(anomalyId: string, status: AnomalyStatus): Promise<Anomaly> {
    const query = this.buildQuery();
    return this.request<Anomaly>(this.path(`/${encodeURIComponent(anomalyId)}/status${query}`), {
      method: "POST",
      body: JSON.stringify({ status })
    });
  }

  /**
   * Run the metric-tree `explain` for an anomaly and cache the result on
   * the row. Subsequent calls return the cached `ExplainResult` instantly;
   * pass `{ refresh: true }` to bust the cache and recompute.
   *
   * The uncached path runs a 20-30 s recursive driver search — budget for it
   * (or read `explain_cache` off the row from {@link list} when it's already
   * populated).
   */
  async explain(anomalyId: string, options: ExplainOptions = {}): Promise<ExplainResult> {
    const extra: Record<string, string> = {};
    if (options.refresh) extra.refresh = "true";
    return this.request<ExplainResult>(
      this.path(`/${encodeURIComponent(anomalyId)}/explain${this.buildQuery(extra)}`),
      { method: "POST" }
    );
  }
}
