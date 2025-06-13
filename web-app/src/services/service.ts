import {
  ThreadCreateRequest,
  ThreadItem,
  Answer,
  MessageItem,
  ThreadsResponse,
} from "@/types/chat";

import { apiService } from "./apiService";
import { AgentConfig } from "@/types/agent";
import { TestStreamMessage } from "@/types/eval";
import { FileTreeModel } from "@/types/file";
import { App, AppItem, Chunk } from "@/types/app";
import { Workflow } from "@/types/workflow";
import { Artifact } from "./mock";
import { DatabaseInfo, DatabaseSyncResponse } from "@/types/database";
import { LogItem } from "./types";

export interface Service {
  listThreads(page?: number, limit?: number): Promise<ThreadsResponse>;
  createThread(request: ThreadCreateRequest): Promise<ThreadItem>;
  deleteThread(threadId: string): Promise<void>;
  bulkDeleteThreads(threadIds: string[]): Promise<void>;
  runApp(pathb64: string): Promise<App>;
  deleteAllThread(): Promise<void>;
  getThread(threadId: string): Promise<ThreadItem>;
  listAgents(): Promise<AgentConfig[]>;
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
    onMessageSent?: () => void,
  ): Promise<void>;
  askTask(
    taskId: string,
    question: string | null,
    onReadStream: (answer: Chunk) => void,
    onMessageSent?: () => void,
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
  listDatabases(): Promise<DatabaseInfo[]>;
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
  getArtifact(id: string): Promise<Artifact>;
  getThreadMessages(threadId: string): Promise<MessageItem[]>;
  syncDatabase(
    database?: string,
    options?: { datasets?: string[] },
  ): Promise<DatabaseSyncResponse>;
  buildDatabase(): Promise<{ success: boolean; message?: string }>;
  runWorkflow(
    pathb64: string,
    onLogItem: (logItem: LogItem) => void,
  ): Promise<void>;
  runWorkflowThread(
    threadId: string,
    onLogItem: (logItem: LogItem) => void,
  ): Promise<void>;
}

export const service = apiService;
