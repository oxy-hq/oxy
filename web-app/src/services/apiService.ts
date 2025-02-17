import axios from "axios";
import { Service } from "./service";
import { apiClient } from "./axios";
import { ChatRequest, ChatType, Message } from "@/types/chat";
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

  async getOpenaiApiKey() {
    return "";
  },

  async setOpenaiApiKey(key) {
    console.log("setOpenaiApiKey", key);
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
};
