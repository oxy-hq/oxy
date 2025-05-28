import {
  ThreadCreateRequest,
  ThreadItem,
  Answer,
  MessageItem,
} from "@/types/chat";

import { apiService } from "./apiService";
import { AgentConfig } from "@/types/agent";
import { TestStreamMessage } from "@/types/eval";
import { FileTreeModel } from "@/types/file";
import { App, AppItem, Chunk } from "@/types/app";
import { Workflow } from "@/types/workflow";

export interface Service {
  listThreads(): Promise<ThreadItem[]>;
  createThread(request: ThreadCreateRequest): Promise<ThreadItem>;
  deleteThread(threadId: string): Promise<void>;
  runApp(pathb64: string): Promise<App>;
  deleteAllThread(): Promise<void>;
  getThread(threadId: string): Promise<ThreadItem>;
  listAgents(): Promise<string[]>;
  listApps(): Promise<AppItem[]>;
  getAgent(pathb64: string): Promise<AgentConfig>;
  getApp(appPath64: string): Promise<App>;
  getData(filePath: string): Promise<Blob>;
  runTestAgent(
    pathb64: string,
    testIndex: number,
    onReadStream: (event: TestStreamMessage) => void,
  ): Promise<void>;
  ask(
    threadId: string,
    question: string | null,
    onReadStream: (answer: Answer) => void,
  ): Promise<void>;
  askTask(
    taskId: string,
    question: string | null,
    onReadStream: (answer: Chunk) => void,
  ): Promise<void>;
  askAgent(
    agentPath: string,
    question: string,
    onReadStream: (answer: Answer) => void,
  ): Promise<void>;
  getChart(file_path: string): Promise<string>;
  getFile(pathb64: string): Promise<string>;
  saveFile(pathb64: string, data: string): Promise<void>;
  executeSql(
    pathb64: string,
    sql: string,
    database: string,
  ): Promise<string[][]>;
  listDatabases(): Promise<string[]>;
  getFileTree(): Promise<FileTreeModel[]>;
  createFile(pathb64: string): Promise<void>;
  createFolder(pathb64: string): Promise<void>;
  deleteFile(pathb64: string): Promise<void>;
  deleteFolder(pathb64: string): Promise<void>;
  renameFile(pathb64: string, newName: string): Promise<void>;
  renameFolder(pathb64: string, newName: string): Promise<void>;
  checkBuilderAvailability(): Promise<{ available: boolean }>;
  createWorkflowFromQuery(request: {
    query: string;
    prompt: string;
    database: string;
  }): Promise<{ workflow: Workflow }>;
  getThreadMessages(threadId: string): Promise<MessageItem[]>;
}

export const service = apiService;
