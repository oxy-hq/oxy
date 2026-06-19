import { encodeBase64 } from "@/libs/encoding";
import type { AppData, AppDisplay, AppItem } from "@/types/app";
import { apiClient } from "./axios";

export class AppService {
  static async listApps(
    projectId: string,
    branchName: string,
    options: { publishedOnly?: boolean } = {}
  ): Promise<AppItem[]> {
    const response = await apiClient.get(`/${projectId}/apps`, {
      params: {
        branch: branchName,
        ...(options.publishedOnly ? { published_only: true } : {})
      }
    });
    return response.data;
  }

  static async publishApp(
    projectId: string,
    branchName: string,
    pathb64: string
  ): Promise<AppItem> {
    const response = await apiClient.post(`/${projectId}/apps/${pathb64}/publish`, null, {
      params: { branch: branchName }
    });
    return response.data;
  }

  static async unpublishApp(
    projectId: string,
    branchName: string,
    pathb64: string
  ): Promise<AppItem> {
    const response = await apiClient.post(`/${projectId}/apps/${pathb64}/unpublish`, null, {
      params: { branch: branchName }
    });
    return response.data;
  }

  static async getAppData(
    projectId: string,
    branchName: string,
    appPath64: string
  ): Promise<AppData> {
    const response = await apiClient.get(`/${projectId}/apps/${appPath64}`, {
      params: { branch: branchName }
    });
    return response.data;
  }

  /** Last cached app data (no execution), served from the compile boundary +
   *  S3 mirror so a stateless serve replica can show a dashboard's last data
   *  when the ide is down. `null` when nothing is cached (the `404`). */
  static async getAppDataCached(
    projectId: string,
    branchName: string,
    appPath64: string
  ): Promise<AppData | null> {
    try {
      const response = await apiClient.get(`/${projectId}/apps/${appPath64}/data-cached`, {
        params: { branch: branchName }
      });
      return response.data;
    } catch {
      return null;
    }
  }

  static async runApp(
    projectId: string,
    branchName: string,
    pathb64: string,
    params: Record<string, unknown> = {}
  ): Promise<AppData> {
    const response = await apiClient.post(
      `/${projectId}/apps/${pathb64}/run`,
      { params },
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
    const response = await apiClient.get(`/${projectId}/apps/${pathb64}/displays`, {
      params: { branch: branchName }
    });
    return response.data;
  }

  static async getData(projectId: string, branchName: string, filePath: string): Promise<Blob> {
    const pathb64 = encodeBase64(filePath);
    const response = await apiClient.get(`/${projectId}/apps/file/${pathb64}`, {
      params: { branch: branchName },
      responseType: "arraybuffer"
    });
    const blob = new Blob([response.data]);
    return blob;
  }
}
