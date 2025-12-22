import { apiClient } from "./axios";
import fetchSSE from "./fetchSSE";
import {
  DatabaseInfo,
  DatabaseSyncResponse,
  WarehousesFormData,
  CreateDatabaseConfigResponse,
  TestDatabaseConnectionRequest,
  ConnectionTestEvent,
} from "@/types/database";

export class DatabaseService {
  static async listDatabases(
    projectId: string,
    branchName: string,
  ): Promise<DatabaseInfo[]> {
    const response = await apiClient.get(`/${projectId}/databases`, {
      params: { branch: branchName },
    });
    return response.data;
  }

  static async executeSql(
    projectId: string,
    branchName: string,
    pathb64: string,
    sql: string,
    database: string,
  ): Promise<string[][]> {
    const response = await apiClient.post(
      `/${projectId}/sql/${pathb64}`,
      {
        sql,
        database,
      },
      { params: { branch: branchName } },
    );
    return response.data;
  }

  static async syncDatabase(
    projectId: string,
    branchName: string,
    database?: string,
    options?: { datasets?: string[] },
  ): Promise<DatabaseSyncResponse> {
    const params = new URLSearchParams();
    params.append("branch", branchName);
    if (database) params.append("database", database);
    if (options?.datasets && options.datasets.length > 0) {
      options.datasets.forEach((dataset) => {
        params.append("datasets", dataset);
      });
    }

    const response = await apiClient.post(
      `/${projectId}/databases/sync?${params.toString()}`,
    );
    return response.data;
  }

  static async buildDatabase(
    projectId: string,
    branchName: string,
  ): Promise<{
    success: boolean;
    message?: string;
  }> {
    const response = await apiClient.post(`/${projectId}/databases/build`, {
      params: { branch: branchName },
    });
    return response.data;
  }

  static async cleanData(
    projectId: string,
    branchName: string,
    target?: string,
  ): Promise<{
    success: boolean;
    message: string;
    cleaned_items: string[];
  }> {
    const params = new URLSearchParams();
    params.append("branch", branchName);
    if (target) params.append("target", target);

    const response = await apiClient.post(
      `/${projectId}/databases/clean?${params.toString()}`,
    );
    return response.data;
  }

  static async createDatabaseConfig(
    projectId: string,
    branchName: string,
    warehouses: WarehousesFormData,
  ): Promise<CreateDatabaseConfigResponse> {
    const response = await apiClient.post(
      `/${projectId}/databases`,
      warehouses,
      {
        params: { branch: branchName },
      },
    );
    return response.data;
  }

  /**
   * Test database connection with SSE streaming for real-time updates
   * @param projectId - The project ID
   * @param branchName - The branch name
   * @param request - Test connection request with warehouse config
   * @param onEvent - Callback to handle streaming events (progress, browser auth, completion)
   * @returns Promise that resolves when the connection test completes
   */
  static async testDatabaseConnection(
    projectId: string,
    branchName: string,
    request: TestDatabaseConnectionRequest,
    onEvent: (event: ConnectionTestEvent) => void,
  ): Promise<void> {
    const baseUrl = apiClient.defaults.baseURL || "";
    const url = `${baseUrl}/${projectId}/databases/test-connection?branch=${encodeURIComponent(branchName)}`;

    await fetchSSE<ConnectionTestEvent>(url, {
      method: "POST",
      body: request,
      onMessage: onEvent,
    });
  }
}
