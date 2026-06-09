import { AxiosError } from "axios";
import type {
  AnomalyStatus,
  ListAnomaliesResponse,
  ListMonitorsResponse,
  MetricAnomaly,
  MonitorEntry,
  ScanAnomaliesResponse
} from "@/types/metricAnomalies";
import type { ExplainResult } from "@/types/metricTree";
import { apiClient } from "./axios";

function rethrow(error: unknown): never {
  if (error instanceof AxiosError && error.response?.data?.message) {
    throw new Error(error.response.data.message);
  }
  throw error;
}

/** Client for the `/semantic/anomalies*` endpoints. */
export class MetricAnomaliesService {
  /** List anomalies for the workspace, optionally filtered by status. */
  static async list(projectId: string, status?: AnomalyStatus): Promise<MetricAnomaly[]> {
    try {
      const response = await apiClient.get<ListAnomaliesResponse>(
        `/${projectId}/semantic/anomalies`,
        { params: status ? { status } : {} }
      );
      return response.data.anomalies;
    } catch (error) {
      rethrow(error);
    }
  }

  /** Run every `.monitor.yml` entry once and upsert detected anomalies.
   *  `asOf` (YYYY-MM-DD) overrides the "now" reference the detector uses —
   *  handy for demos against datasets that don't include the current date. */
  static async scan(projectId: string, asOf?: string): Promise<ScanAnomaliesResponse> {
    try {
      const response = await apiClient.post<ScanAnomaliesResponse>(
        `/${projectId}/semantic/anomalies/scan`,
        null,
        { params: asOf ? { as_of: asOf } : {} }
      );
      return response.data;
    } catch (error) {
      rethrow(error);
    }
  }

  /** Run (or return cached) airlayer explain for a single anomaly. Cache
   *  lives on the row itself, so reopening the same anomaly across page
   *  refreshes returns instantly. `refresh=true` busts the cache. */
  static async explain(
    projectId: string,
    anomalyId: string,
    refresh = false
  ): Promise<ExplainResult> {
    try {
      const response = await apiClient.post<ExplainResult>(
        `/${projectId}/semantic/anomalies/${anomalyId}/explain`,
        null,
        { params: refresh ? { refresh: "true" } : {} }
      );
      return response.data;
    } catch (error) {
      rethrow(error);
    }
  }

  /** List all entries from `.monitor.yml` for the workspace. Returns an
   *  empty array when no file is configured; returns an error when the
   *  file exists but fails to parse. */
  static async listMonitors(projectId: string): Promise<MonitorEntry[]> {
    try {
      const response = await apiClient.get<ListMonitorsResponse>(`/${projectId}/semantic/monitors`);
      return response.data.monitors;
    } catch (error) {
      rethrow(error);
    }
  }

  /** Move an anomaly between new / acknowledged / dismissed. */
  static async updateStatus(
    projectId: string,
    anomalyId: string,
    status: AnomalyStatus
  ): Promise<MetricAnomaly> {
    try {
      const response = await apiClient.post<MetricAnomaly>(
        `/${projectId}/semantic/anomalies/${anomalyId}/status`,
        { status }
      );
      return response.data;
    } catch (error) {
      rethrow(error);
    }
  }
}
