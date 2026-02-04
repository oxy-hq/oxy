import type { AppData, AppDisplay, AppItem } from "@/types/app";
import { apiClient } from "./axios";

export class AppService {
  static async listApps(projectId: string, branchName: string): Promise<AppItem[]> {
    const response = await apiClient.get(`/${projectId}/app`, {
      params: { branch: branchName }
    });
    return response.data;
  }

  static async getAppData(
    projectId: string,
    branchName: string,
    appPath64: string
  ): Promise<AppData> {
    const response = await apiClient.get(`/${projectId}/app/${appPath64}`, {
      params: { branch: branchName }
    });
    return response.data;
  }

  static async runApp(projectId: string, branchName: string, pathb64: string): Promise<AppData> {
    const response = await apiClient.post(
      `/${projectId}/app/${pathb64}/run`,
      {},
      {
        params: { branch: branchName }
      }
    );
    return response.data;
  }

  static async getDisplays(
    projectId: string,
    branchName: string,
    pathb64: string
  ): Promise<AppDisplay> {
    const response = await apiClient.get(`/${projectId}/app/${pathb64}/displays`, {
      params: { branch: branchName }
    });
    return response.data;
  }

  static async getData(projectId: string, branchName: string, filePath: string): Promise<Blob> {
    const pathb64 = btoa(filePath);
    const response = await apiClient.get(`/${projectId}/app/file/${pathb64}`, {
      params: { branch: branchName },
      responseType: "arraybuffer"
    });
    const blob = new Blob([response.data]);
    return blob;
  }
}
