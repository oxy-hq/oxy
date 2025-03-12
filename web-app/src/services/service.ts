import {
  ChatRequest,
  ChatType,
  Message,
  ThreadCreateRequest,
  ThreadItem,
  Answer,
} from "@/types/chat";

import { apiService } from "./apiService";

export interface Service {
  listThreads(): Promise<ThreadItem[]>;
  createThread(request: ThreadCreateRequest): Promise<ThreadItem>;
  getThread(threadId: string): Promise<ThreadItem>;
  listChatMessages(agentPath: string): Promise<Message[]>;
  getOpenaiApiKey(): Promise<string>;
  setOpenaiApiKey(key: string): Promise<void>;
  listAgents(): Promise<string[]>;
  chat(
    type: ChatType,
    request: ChatRequest,
    onReadStream: (message: Message) => void,
    abortSignal: AbortSignal,
  ): Promise<void>;

  ask(threadId: string, onReadStream: (answer: Answer) => void): Promise<void>;
}

export const service = apiService;
