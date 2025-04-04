import { ThreadCreateRequest, ThreadItem, Answer } from "@/types/chat";

import { apiService } from "./apiService";

export interface Service {
  listThreads(): Promise<ThreadItem[]>;
  createThread(request: ThreadCreateRequest): Promise<ThreadItem>;
  deleteThread(threadId: string): Promise<void>;
  deleteAllThread(): Promise<void>;
  getThread(threadId: string): Promise<ThreadItem>;
  listAgents(): Promise<string[]>;

  ask(threadId: string, onReadStream: (answer: Answer) => void): Promise<void>;
}

export const service = apiService;
