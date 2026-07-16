import type { AssumeSession } from "@/types/adminAssume";
import { apiClient } from "./axios";

/**
 * Explicit assume-role (`/api/assume`). Without a live session, Oxy staff
 * get NO synthetic Owner membership in a tenant — the seam is closed by default.
 */
export const AdminAssumeService = {
  /** Begin acting as an Owner of `orgId`. A reason is required. */
  async start(orgId: string, reason: string): Promise<AssumeSession> {
    const { data } = await apiClient.post("/assume", {
      org_id: orgId,
      reason
    });
    return data;
  },

  /** Stop. Omit `orgId` to end every live session. */
  async end(orgId?: string): Promise<void> {
    await apiClient.delete("/assume", {
      params: orgId ? { org_id: orgId } : undefined
    });
  },

  /** The caller's live sessions — drives the banner. */
  async current(): Promise<AssumeSession[]> {
    const { data } = await apiClient.get("/assume/current");
    return data;
  },

  /** Every session ever — the impersonation log. */
  async history(): Promise<AssumeSession[]> {
    const { data } = await apiClient.get("/assume/history");
    return data;
  }
};
