import { App, AppItem } from "./../types/app";
import { Service } from "./service";
import { apiClient } from "./axios";
import { readMessageFromStreamData } from "@/libs/utils/stream";
import { apiBaseURL } from "./env";
import { ThreadCreateRequest } from "@/types/chat";
import { TestStreamMessage } from "@/types/eval";
import { Workflow } from "@/types/workflow";

export const apiService: Service = {
  async listThreads(page?: number, limit?: number) {
    const params = new URLSearchParams();
    if (page !== undefined) params.append("page", page.toString());
    if (limit !== undefined) params.append("limit", limit.toString());

    let url = "/threads";
    const paramsStr = params.toString();
    if (paramsStr) {
      url += "?" + paramsStr;
    }
    const response = await apiClient.get(url);
    return response.data;
  },
  async deleteThread(threadId: string) {
    const response = await apiClient.delete(`/threads/${threadId}`);
    return response.data;
  },
  async deleteAllThread() {
    const response = await apiClient.delete("/threads");
    return response.data;
  },
  async getThread(threadId: string) {
    const response = await apiClient.get(`/threads/${threadId}`);
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
  async ask(threadId: string, question: string | null, onReadStream) {
    const url = `/threads/${threadId}/ask`;
    const options = {
      headers: {
        "Content-Type": "application/json",
      },
      body: JSON.stringify({
        question: question,
      }),
      method: "POST",
    };
    const response = await fetch(apiBaseURL + url, options);
    if (response) {
      await readMessageFromStreamData(response, onReadStream);
    }
  },
  async askTask(threadId: string, question: string | null, onReadStream) {
    const url = `/threads/${threadId}/task`;
    const options = {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
      },
      body: JSON.stringify({
        question: question,
      }),
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
  async getFile(pathb64: string) {
    const response = await apiClient.get("/files/" + pathb64);
    return response.data;
  },
  async saveFile(pathb64: string, data: string) {
    const response = await apiClient.post("/files/" + pathb64, { data });
    return response.data;
  },
  async executeSql(pathb64: string, sql: string, database: string) {
    const response = await apiClient.post("/sql/" + pathb64, {
      sql,
      database,
    });
    return response.data;
  },
  async listDatabases() {
    const response = await apiClient.get("/databases");
    return response.data;
  },
  async getFileTree() {
    const response = await apiClient.get("/files");
    return response.data;
  },
  async askAgent(agentPathb64: string, question: string, onReadStream) {
    const url = `/agents/${agentPathb64}/ask`;
    const options = {
      method: "POST",
      body: JSON.stringify({ question }),
      headers: {
        "Content-Type": "application/json",
      },
    };
    const response = await fetch(apiBaseURL + url, options);
    if (response) {
      await readMessageFromStreamData(response, onReadStream);
    }
  },
  async listApps(): Promise<AppItem[]> {
    const response = await apiClient.get("/apps");
    return response.data;
  },
  async getApp(appPath64: string): Promise<App> {
    const response = await apiClient.get("/app/" + appPath64);
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
  async runApp(pathb64: string): Promise<App> {
    const response = await apiClient.post(`/app/${pathb64}/run`);
    return response.data;
  },
  checkBuilderAvailability: async function (): Promise<{ available: boolean }> {
    const response = await apiClient.get("/builder-availability");
    return response.data;
  },
  async createFile(pathb64: string): Promise<void> {
    const response = await apiClient.post(`/files/${pathb64}/new-file`);
    return response.data;
  },
  async createFolder(pathb64: string): Promise<void> {
    const response = await apiClient.post(`/files/${pathb64}/new-folder`);
    return response.data;
  },
  async deleteFile(pathb64: string): Promise<void> {
    const response = await apiClient.delete(`/files/${pathb64}/delete-file`);
    return response.data;
  },
  async deleteFolder(pathb64: string): Promise<void> {
    const response = await apiClient.delete(`/files/${pathb64}/delete-folder`);
    return response.data;
  },
  async renameFile(pathb64: string, newName: string): Promise<void> {
    const response = await apiClient.put(`/files/${pathb64}/rename-file`, {
      new_name: newName,
    });
    return response.data;
  },
  async renameFolder(pathb64: string, newName: string): Promise<void> {
    const response = await apiClient.put(`/files/${pathb64}/rename-folder`, {
      new_name: newName,
    });
    return response.data;
  },
  createWorkflowFromQuery: async function (request: {
    query: string;
    prompt: string;
    database: string;
  }): Promise<{ workflow: Workflow }> {
    const response = await apiClient.post("/workflows/from-query", request);
    return response.data;
  },
  async getThreadMessages(threadId: string) {
    const response = await apiClient.get(`/threads/${threadId}/messages`);
    return response.data;
  },
};
