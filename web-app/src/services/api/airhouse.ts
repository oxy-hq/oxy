import { apiClient } from "./axios";

export type AirhouseConnectionInfo = {
  host: string;
  port: number;
  dbname: string;
  username: string;
  /** True until the user has surfaced the password via /credentials at least once.
   *  The password stays retrievable regardless — used only for "first reveal" UI cues. */
  password_not_yet_shown: boolean;
};

export type AirhouseCredentials = {
  username: string;
  host: string;
  port: number;
  dbname: string;
  role: string;
  status: string;
  password?: string;
  password_already_revealed: boolean;
};

export const AirhouseService = {
  async getConnection(workspaceId: string): Promise<AirhouseConnectionInfo> {
    const response = await apiClient.get("/airhouse/me/connection", {
      params: { workspace_id: workspaceId }
    });
    return response.data;
  },

  async revealCredentials(workspaceId: string): Promise<AirhouseCredentials> {
    const response = await apiClient.get("/airhouse/me/credentials", {
      params: { workspace_id: workspaceId }
    });
    return response.data;
  },

  async provision(workspaceId: string, tenantName: string): Promise<AirhouseConnectionInfo> {
    const response = await apiClient.post(
      "/airhouse/me/provision",
      { tenant_name: tenantName },
      { params: { workspace_id: workspaceId } }
    );
    return response.data;
  },

  async rotatePassword(workspaceId: string): Promise<AirhouseConnectionInfo> {
    const response = await apiClient.post("/airhouse/me/rotate-password", null, {
      params: { workspace_id: workspaceId }
    });
    return response.data;
  }
};
