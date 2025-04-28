import { App, AppItem } from "./../types/app";
import { Service } from "./service";
import { apiClient } from "./axios";
import { readMessageFromStreamData } from "@/libs/utils/stream";
import { apiBaseURL } from "./env";
import { ThreadCreateRequest } from "@/types/chat";
import { TestStreamMessage } from "@/types/eval";

export const apiService: Service = {
  async listThreads() {
    const response = await apiClient.get("/threads");
    return response.data;
  },
  async deleteThread(threadId: string) {
    const response = await apiClient.delete("/threads/" + threadId);
    return response.data;
  },
  async deleteAllThread() {
    const response = await apiClient.delete("/threads");
    return response.data;
  },
  async getThread(threadId: string) {
    const response = await apiClient.get("/threads/" + threadId);
    return response.data;
  },
  async createThread(request: ThreadCreateRequest) {
    const response = await apiClient.post("/threads", request);
    return response.data;
  },
  async listAgents() {
    const response = await apiClient.get("/agents");
    return response.data;
  },
  async runTestAgent(
    pathb64: string,
    testIndex: number,
    onReadStream: (event: TestStreamMessage) => void,
  ) {
    const url = `/agents/${pathb64}/tests/${testIndex}`;
    const options = {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
      },
    };
    const response = await fetch(apiBaseURL + url, options);
    if (response) {
      await readMessageFromStreamData(response, onReadStream);
    }
  },
  async ask(threadId: string, onReadStream) {
    const url = `/threads/${threadId}/ask`;
    const options = {
      headers: {
        "Content-Type": "application/json",
      },
    };
    const response = await fetch(apiBaseURL + url, options);
    if (response) {
      await readMessageFromStreamData(response, onReadStream);
    }
  },
  async getAgent(pathb64: string) {
    const response = await apiClient.get("/agents/" + pathb64);
    return response.data;
  },
  async getChart(file_path: string) {
    const response = await apiClient.get("/charts/" + file_path);
    return response.data;
  },
  async listApps(): Promise<AppItem[]> {
    const response = await apiClient.get("/apps");
    return response.data;
  },
  async getApp(appPath: string): Promise<App> {
    const pathb64 = btoa(appPath);
    const response = await apiClient.get("/app/" + pathb64);
    return response.data;
  },
  async getData(filePath: string): Promise<Blob> {
    const pathb64 = btoa(filePath);
    const response = await apiClient.get("/app/file/" + pathb64, {
      responseType: "arraybuffer",
    });
    const blob = new Blob([response.data]);

    return blob;
  },
  async runApp(appPath: string): Promise<App> {
    const pathb64 = btoa(appPath);
    const response = await apiClient.post(`/app/${pathb64}/run`);
    return response.data;
  },
};
