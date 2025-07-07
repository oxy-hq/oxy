import { apiClient } from "./axios";
import {
  ThreadCreateRequest,
  ThreadItem,
  ThreadsResponse,
  Answer,
  Message,
} from "@/types/chat";
import fetchSSE from "./fetchSSE";
import { apiBaseURL } from "../env";

export class ThreadService {
  static async listThreads(
    page?: number,
    limit?: number,
  ): Promise<ThreadsResponse> {
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
  }

  static async createThread(request: ThreadCreateRequest): Promise<ThreadItem> {
    const response = await apiClient.post("/threads", request);
    return response.data;
  }

  static async getThread(threadId: string): Promise<ThreadItem> {
    const response = await apiClient.get(`/threads/${threadId}`);
    return response.data;
  }

  static async deleteThread(threadId: string): Promise<void> {
    const response = await apiClient.delete(`/threads/${threadId}`);
    return response.data;
  }

  static async bulkDeleteThreads(threadIds: string[]): Promise<void> {
    const response = await apiClient.post("/threads/bulk-delete", {
      thread_ids: threadIds,
    });
    return response.data;
  }

  static async deleteAllThreads(): Promise<void> {
    const response = await apiClient.delete("/threads");
    return response.data;
  }

  static async getThreadMessages(threadId: string): Promise<Message[]> {
    const response = await apiClient.get(`/threads/${threadId}/messages`);
    return response.data;
  }

  static async askTask(
    taskId: string,
    question: string | null,
    onReadStream: (answer: Answer) => void,
    onMessageSent?: () => void,
  ): Promise<void> {
    const url = `${apiBaseURL}/threads/${taskId}/task`;
    await fetchSSE(url, {
      body: { question },
      onMessage: onReadStream,
      onOpen: onMessageSent,
      eventTypes: ["message", "error"],
    });
  }

  static async askAgent(
    threadId: string,
    question: string | null,
    onReadStream: (answer: Answer) => void,
    onMessageSent?: () => void,
  ): Promise<void> {
    const url = `${apiBaseURL}/threads/${threadId}/agent`;
    await fetchSSE(url, {
      body: { question },
      onMessage: onReadStream,
      onOpen: onMessageSent,
      eventTypes: ["message", "error"],
    });
  }

  static async stopThread(threadId: string): Promise<void> {
    const url = `${apiBaseURL}/threads/${threadId}/stop`;
    await apiClient.post(url);
  }
}
