import axios from "axios";
import { Service } from "./service";
import { apiClient } from "./axios";
import {
  ChatRequest,
  ChatType,
  Message,
  ThreadCreateRequest,
} from "@/types/chat";
import { readMessageFromStreamData } from "@/libs/utils/stream";
import { apiBaseURL } from "./env";

export const apiService: Service = {
  async listChatMessages(agentPath) {
    try {
      const response = await apiClient.get("/messages/" + agentPath);
      return response.data;
    } catch (error) {
      if (axios.isAxiosError(error) && error.response?.status === 404) {
        return [];
      }
      throw error;
    }
  },
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
  async getOpenaiApiKey() {
    return "";
  },

  async setOpenaiApiKey(key) {
    console.log("setOpenaiApiKey", key);
  },
  async listAgents() {
    const response = await apiClient.get("/agents");
    return response.data;
  },

  async chat(
    type: ChatType,
    request: ChatRequest,
    onReadStream: (message: Message) => void,
    abortSignal: AbortSignal,
  ) {
    const url = type === "chat" ? "/ask" : "/preview";
    const options = {
      body: JSON.stringify(request),
      signal: abortSignal,
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
};
