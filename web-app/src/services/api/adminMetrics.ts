import { apiClient } from "./axios";

export interface UsageTotals {
  cost_usd: number;
  input_tokens: number;
  output_tokens: number;
  cache_creation_tokens: number;
  cache_read_tokens: number;
  run_count: number;
  priced_run_count: number;
}

export interface DayCost {
  day: string;
  cost_usd: number;
  input_tokens: number;
  output_tokens: number;
  run_count: number;
}

export interface ModelCost {
  model: string;
  /** null when the model isn't in the pricing table (tokens still reported). */
  cost_usd: number | null;
  input_tokens: number;
  output_tokens: number;
  cache_creation_tokens: number;
  cache_read_tokens: number;
  run_count: number;
}

export interface OrgCost {
  org_id: string;
  org_name: string;
  org_slug: string;
  cost_usd: number;
  input_tokens: number;
  output_tokens: number;
  run_count: number;
}

export interface LlmUsageOverview {
  window_days: number;
  total: UsageTotals;
  by_day: DayCost[];
  by_model: ModelCost[];
  by_org: OrgCost[];
}

export const AdminMetricsService = {
  async llmUsage(days = 30): Promise<LlmUsageOverview> {
    const res = await apiClient.get<LlmUsageOverview>("/admin/metrics/llm-usage", {
      params: { days }
    });
    return res.data;
  }
};
