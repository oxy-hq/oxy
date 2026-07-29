/**
 * Anomaly Inbox types — mirror the Rust `entity::metric_anomalies::Model`
 * and the `metric_anomalies` HTTP responses.
 */

export type AnomalySeverity = "low" | "medium" | "high";
export type AnomalyStatus = "new" | "acknowledged" | "dismissed";

/** A single dimension filter identifying which segment an anomaly belongs to. */
export interface AnomalyFilter {
  /** Fully-qualified dimension id, e.g. `"labor_daily.restaurant_id"`. */
  member: string;
  /** Matched values (OR within a filter). */
  values: string[];
}

export interface MetricAnomaly {
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
  severity: AnomalySeverity;
  status: AnomalyStatus;
  label: string | null;
  /**
   * Stable key derived from the monitor's filters (e.g.
   * `"labor_daily.restaurant_id=loc-abc"`). Empty string for chain-wide
   * (unfiltered) monitors. Distinguishes per-segment anomalies that share a
   * measure/period so they don't read as duplicates.
   */
  dimension_key: string;
  /** Raw filters identifying this anomaly's segment. Null for chain-wide monitors. */
  filters: AnomalyFilter[] | null;
  /**
   * Groups consecutive flagged buckets of one segment into a single event, so a
   * surge spanning Mon/Wed/Thu reads as one problem rather than three. Rows stay
   * per-bucket (explain reasons about a single bucket), so the collapsing
   * happens here on read. Null for rows detected before events existed.
   */
  event_id: string | null;
  detected_at: string;
  updated_at: string;
}

export interface ListAnomaliesResponse {
  anomalies: MetricAnomaly[];
}

/** A monitor that errored during a scan — identifies the monitor/segment and the error. */
export interface ScanFailure {
  measure: string;
  time_dimension: string;
  granularity: string;
  label: string | null;
  /** Segment key when the failed monitor was a group_by/filtered segment; empty for chain-wide. */
  dimension_key: string;
  /** Raw filters identifying the segment; null for chain-wide monitors. */
  filters: AnomalyFilter[] | null;
  error: string;
}

export interface ScanAnomaliesResponse {
  monitors_scanned: number;
  monitors_failed: number;
  anomalies_persisted: number;
  /** True when the scan is still running in the background. Refetch anomalies after a short delay. */
  pending?: boolean;
  /** Per-monitor failures. Empty on a clean scan and on the `pending` path. */
  failures?: ScanFailure[];
}

export interface MonitorEntry {
  measure: string;
  time_dimension: string;
  granularity: "day" | "week" | "month";
  lookback_days: number;
  seasonality: number[] | null;
  sensitivity: "low" | "medium" | "high";
  label?: string | null;
  /**
   * Dimension filters narrowing this entry to one segment. Two entries over the
   * same measure/time-dimension/granularity are distinguished only by these, so
   * anything mapping coverage rows back to an entry must use them.
   */
  filters?: AnomalyFilter[];
  /**
   * Fan-out dimension: the scanner discovers its values at scan time and files
   * one coverage row per segment, each keyed by this entry's `filters` *plus*
   * the discovered value.
   */
  group_by?: string | null;
}

/** Per-segment scan coverage. A `group_by` monitor fans out to one row per
 *  segment, so a single `MonitorEntry` can map to many of these.
 *
 *  Exists because a monitor skipped for want of history produces neither an
 *  anomaly nor a failure — without this the UI cannot tell "healthy, nothing
 *  found" from "not scoring at all". */
export interface MonitorCoverage {
  id: string;
  workspace_id: string;
  measure: string;
  time_dimension: string;
  granularity: string;
  /** Empty string for chain-wide monitors. */
  dimension_key: string;
  filters: Record<string, unknown>[] | null;
  label: string | null;
  /** Buckets the warehouse returned; zero-filled gaps are not counted. */
  measured_buckets: number;
  /** The statistical floor this segment must clear to be scored. */
  required_buckets: number;
  last_scanned_at: string;
}

export interface ListMonitorsResponse {
  monitors: MonitorEntry[];
  coverage: MonitorCoverage[];
}
