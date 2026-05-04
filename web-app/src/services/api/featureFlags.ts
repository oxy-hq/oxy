import { apiClient } from "./axios";

export interface FeatureFlag {
  key: string;
  description: string;
  default: boolean;
  enabled: boolean;
  updated_at: string | null;
}

export class FeatureFlagsService {
  static async list(): Promise<FeatureFlag[]> {
    const response = await apiClient.get<FeatureFlag[]>("/admin/feature-flags");
    return response.data;
  }

  static async update(key: string, enabled: boolean): Promise<FeatureFlag> {
    const response = await apiClient.patch<FeatureFlag>(
      `/admin/feature-flags/${encodeURIComponent(key)}`,
      { enabled }
    );
    return response.data;
  }
}
