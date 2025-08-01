import { apiClient } from "./axios";
import { AppData, AppDisplay, AppItem } from "@/types/app";

export class AppService {
  static async listApps(): Promise<AppItem[]> {
    const response = await apiClient.get("/apps");
    return response.data;
  }

  static async getAppData(appPath64: string): Promise<AppData> {
    const response = await apiClient.get("/app/" + appPath64);
    return response.data;
  }

  static async runApp(pathb64: string): Promise<AppData> {
    const response = await apiClient.post(`/app/${pathb64}/run`);
    return response.data;
  }

  static async getDisplays(pathb64: string): Promise<AppDisplay> {
    const response = await apiClient.get(`/app/${pathb64}/displays`);
    return response.data;
  }

  static async getData(filePath: string): Promise<Blob> {
    const pathb64 = btoa(filePath);
    const response = await apiClient.get("/app/file/" + pathb64, {
      responseType: "arraybuffer",
    });
    const blob = new Blob([response.data]);
    return blob;
  }
}
