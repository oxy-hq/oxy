import { apiClient } from "./axios";
import { DatabaseInfo, DatabaseSyncResponse } from "@/types/database";

export class DatabaseService {
  static async listDatabases(): Promise<DatabaseInfo[]> {
    const response = await apiClient.get("/databases");
    return response.data;
  }

  static async executeSql(
    pathb64: string,
    sql: string,
    database: string,
  ): Promise<string[][]> {
    const response = await apiClient.post("/sql/" + pathb64, {
      sql,
      database,
    });
    return response.data;
  }

  static async syncDatabase(
    database?: string,
    options?: { datasets?: string[] },
  ): Promise<DatabaseSyncResponse> {
    const params = new URLSearchParams();
    if (database) params.append("database", database);
    if (options?.datasets && options.datasets.length > 0) {
      options.datasets.forEach((dataset) => {
        params.append("datasets", dataset);
      });
    }

    const response = await apiClient.post(
      `/databases/sync?${params.toString()}`,
    );
    return response.data;
  }

  static async buildDatabase(): Promise<{
    success: boolean;
    message?: string;
  }> {
    const response = await apiClient.post("/databases/build");
    return response.data;
  }

  static async cleanData(target?: string): Promise<{
    success: boolean;
    message: string;
    cleaned_items: string[];
  }> {
    const params = new URLSearchParams();
    if (target) params.append("target", target);

    const response = await apiClient.post(
      `/databases/clean?${params.toString()}`,
    );
    return response.data;
  }
}
