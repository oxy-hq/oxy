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
}

export interface ListMonitorsResponse {
  monitors: MonitorEntry[];
}
