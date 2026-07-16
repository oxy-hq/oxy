import type { AppAdmin, OrgSubdomainStatus, OxyAccessStatus } from "@/types/access";
import { apiClient } from "./axios";

/**
 * App-admins (global role) — gated by OXY_OWNER. Mounted at
 * `/api/admin/app-admins`.
 */
export const AppAdminsService = {
  async list(): Promise<AppAdmin[]> {
    const response = await apiClient.get("/admin/app-admins");
    return response.data;
  },

  async create(email: string): Promise<AppAdmin> {
    const response = await apiClient.post("/admin/app-admins", { email });
    return response.data;
  },

  async remove(id: string): Promise<void> {
    await apiClient.delete(`/admin/app-admins/${id}`);
  }
};

/**
 * Per-workspace "let Oxy build tailored apps on our data" toggle. Gated
 * by workspace owner (i.e. org owner). Mounted at
 * `/api/{workspaceId}/oxy-access` — the workspace-context middleware
 * handles authorization in one place.
 */
export const OxyAccessService = {
  async get(workspaceId: string): Promise<OxyAccessStatus> {
    const response = await apiClient.get(`/${workspaceId}/oxy-access`);
    return response.data;
  },

  /** Lock Oxy staff OUT of this workspace. */
  async lock(workspaceId: string): Promise<OxyAccessStatus> {
    const response = await apiClient.post(`/${workspaceId}/oxy-access`);
    return response.data;
  },

  /** Lift the lockdown — restores the default (Oxy staff may access). */
  async unlock(workspaceId: string): Promise<void> {
    await apiClient.delete(`/${workspaceId}/oxy-access`);
  }
};

/**
 * Per-org bare subdomain (`<org-slug>.<zone>`) — READ-ONLY status for the
 * customer's settings. Owner-readable; mounted at
 * `/api/{workspaceId}/org-subdomain`. Enable/disable is an Oxy-staff action
 * in the admin panel (see `AdminOrgsService.setSubdomain`).
 */
export const OrgSubdomainService = {
  async get(workspaceId: string): Promise<OrgSubdomainStatus> {
    const response = await apiClient.get(`/${workspaceId}/org-subdomain`);
    return response.data;
  }
};
