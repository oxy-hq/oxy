import { ChatRequest, ChatType, Message } from "@/types/chat";

import { apiService } from "./apiService";

export interface Service {
  listChatMessages(agentPath: string): Promise<Message[]>;
  getOpenaiApiKey(): Promise<string>;
  setOpenaiApiKey(key: string): Promise<void>;
  chat(
    type: ChatType,
    request: ChatRequest,
    onReadStream: (message: Message) => void,
    abortSignal: AbortSignal,
  ): Promise<void>;
}

export const service = apiService;
