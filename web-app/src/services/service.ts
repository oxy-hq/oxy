import { ThreadCreateRequest, ThreadItem, Answer } from "@/types/chat";

import { apiService } from "./apiService";
import { AgentConfig } from "@/types/agent";
import { TestStreamMessage } from "@/types/eval";
import { App, AppItem } from "@/types/app";

export interface Service {
  listThreads(): Promise<ThreadItem[]>;
  createThread(request: ThreadCreateRequest): Promise<ThreadItem>;
  deleteThread(threadId: string): Promise<void>;
  runApp(appPath: string): Promise<App>;
  deleteAllThread(): Promise<void>;
  getThread(threadId: string): Promise<ThreadItem>;
  listAgents(): Promise<string[]>;
  listApps(): Promise<AppItem[]>;
  getAgent(pathb64: string): Promise<AgentConfig>;
  getApp(appPath: string): Promise<App>;
  getData(filePath: string): Promise<Blob>;
  runTestAgent(
    pathb64: string,
    testIndex: number,
    onReadStream: (event: TestStreamMessage) => void,
  ): Promise<void>;
  ask(threadId: string, onReadStream: (answer: Answer) => void): Promise<void>;
  getChart(file_path: string): Promise<string>;
}

export const service = apiService;
