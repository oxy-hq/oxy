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

  /**
   * The impersonation log, newest first.
   *
   * ONE PAGE, not every session ever: `admin_assume_sessions` is append-only
   * and spans every tenant, so the endpoint answers with the most recent 100
   * (500 max) and takes `limit`/`offset` for the rest. A caller that renders
   * this as "the whole log" is wrong about it.
   */
  async history(params?: { limit?: number; offset?: number }): Promise<AssumeSession[]> {
    const { data } = await apiClient.get("/assume/history", { params });
    return data;
  }
};
