import { ChatRequest, ChatType, Message } from "@/types/chat";

import { apiService } from "./apiService";
import { tauriService } from "./tauriService";

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

declare global {
  interface Window {
    __TAURI__: unknown;
  }
}

const isTauri = !!window.__TAURI__;

export const service = isTauri ? tauriService : apiService;
