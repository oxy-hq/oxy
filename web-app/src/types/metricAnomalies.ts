/**
 * Anomaly Inbox types — mirror the Rust `entity::metric_anomalies::Model`
 * and the `metric_anomalies` HTTP responses.
 */

export type AnomalySeverity = "low" | "medium" | "high";
export type AnomalyStatus = "new" | "acknowledged" | "dismissed";

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
  detected_at: string;
  updated_at: string;
}

export interface ListAnomaliesResponse {
  anomalies: MetricAnomaly[];
}

export interface ScanAnomaliesResponse {
  monitors_scanned: number;
  monitors_failed: number;
  anomalies_persisted: number;
  /** True when the scan is still running in the background. Refetch anomalies after a short delay. */
  pending?: boolean;
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
