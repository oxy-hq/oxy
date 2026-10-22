import type { ReleaseTimeline } from "@/types/appReleases";
import { apiClient } from "./axios";

export class AppReleasesService {
  /**
   * An app's releases over the last `days` (1, 7 or 30 here; the server takes
   * 1–30), each beside the one before it. Derived on every read.
   */
  static async list(appId: string, days: number): Promise<ReleaseTimeline> {
    const response = await apiClient.get<ReleaseTimeline>(
      `/customer-apps/${appId}/releases?days=${days}`
    );
    return response.data;
  }
}
