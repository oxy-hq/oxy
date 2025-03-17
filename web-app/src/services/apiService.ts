import { Service } from "./service";
import { apiClient } from "./axios";
import { readMessageFromStreamData } from "@/libs/utils/stream";
import { apiBaseURL } from "./env";
import { ThreadCreateRequest } from "@/types/chat";

export const apiService: Service = {
  async listThreads() {
    const response = await apiClient.get("/threads");
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
};
