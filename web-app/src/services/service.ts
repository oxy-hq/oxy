import {
  ThreadCreateRequest,
  ThreadItem,
  Answer,
  TaskCreateRequest,
  TaskItem,
} from "@/types/chat";

import { apiService } from "./apiService";
import { AgentConfig } from "@/types/agent";
import { TestStreamMessage } from "@/types/eval";
import { FileTreeModel } from "@/types/file";
import { App, AppItem, Chunk } from "@/types/app";

export interface Service {
  listThreads(): Promise<ThreadItem[]>;
  listTasks(): Promise<TaskItem[]>;
  createThread(request: ThreadCreateRequest): Promise<ThreadItem>;
  createTask(request: TaskCreateRequest): Promise<TaskItem>;
  deleteThread(threadId: string): Promise<void>;
  runApp(pathb64: string): Promise<App>;
  deleteAllThread(): Promise<void>;
  deleteAllTasks(): Promise<void>;
  getThread(threadId: string): Promise<ThreadItem>;
  getTask(taskId: string): Promise<TaskItem>;
  deleteTask(taskId: string): Promise<void>;
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
  ask(threadId: string, onReadStream: (answer: Answer) => void): Promise<void>;
  askTask(taskId: string, onReadStream: (answer: Chunk) => void): Promise<void>;
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
  checkBuilderAvailability(): Promise<{ available: boolean }>;
}

export const service = apiService;
