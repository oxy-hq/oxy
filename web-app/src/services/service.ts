import { ThreadCreateRequest, ThreadItem, Answer } from "@/types/chat";

import { apiService } from "./apiService";
import { AgentConfig } from "@/types/agent";
import { TestStreamMessage } from "@/types/eval";

export interface Service {
  listThreads(): Promise<ThreadItem[]>;
  createThread(request: ThreadCreateRequest): Promise<ThreadItem>;
  deleteThread(threadId: string): Promise<void>;
  deleteAllThread(): Promise<void>;
  getThread(threadId: string): Promise<ThreadItem>;
  listAgents(): Promise<string[]>;
  getAgent(pathb64: string): Promise<AgentConfig>;
  runTestAgent(
    pathb64: string,
    testIndex: number,
    onReadStream: (event: TestStreamMessage) => void,
  ): Promise<void>;
  ask(threadId: string, onReadStream: (answer: Answer) => void): Promise<void>;
  getChart(file_path: string): Promise<string>;
}

export const service = apiService;
