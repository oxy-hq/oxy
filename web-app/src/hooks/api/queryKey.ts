import type { PaginationState } from "@tanstack/react-table";

const agentKeys = {
  all: ["agent"] as const,
  list: (projectId: string, branchName: string) =>
    [...agentKeys.all, "list", projectId, branchName] as const,
  get: (pathb64: string, projectId: string, branchName: string) =>
    [...agentKeys.all, "get", pathb64, projectId, branchName] as const
};

const analyticsKeys = {
  all: ["analytics"] as const,
  runByThread: (projectId: string, threadId: string) =>
    [...analyticsKeys.all, "runByThread", projectId, threadId] as const,
  runsByThread: (projectId: string, threadId: string) =>
    [...analyticsKeys.all, "runsByThread", projectId, threadId] as const
};

const threadKeys = {
  all: ["thread"] as const,
  list: (projectId: string, page?: number, limit?: number) =>
    [...threadKeys.all, "list", projectId, { page, limit }] as const,
  item: (projectId: string, threadId: string) =>
    [...threadKeys.all, projectId, { threadId }] as const,
  messages: (projectId: string, threadId: string) =>
    [...threadKeys.all, "messages", projectId, threadId] as const
};

const traceKeys = {
  all: ["trace"] as const,
  list: (projectId: string, limit?: number, offset?: number, status?: string, duration?: string) =>
    [...traceKeys.all, "list", projectId, { limit, offset, status, duration }] as const,
  item: (projectId: string, traceId: string) => [...traceKeys.all, projectId, { traceId }] as const
};

const workflowKeys = {
  all: ["workflow"] as const,
  run: (projectId: string, branchName: string) =>
    [...workflowKeys.all, "run", projectId, branchName] as const,
  list: (projectId: string, branchName: string) =>
    [...workflowKeys.all, "list", projectId, branchName] as const,
  get: (projectId: string, branchName: string, relative_path: string) =>
    [...workflowKeys.all, "get", projectId, branchName, relative_path] as const,
  getLogs: (projectId: string, branchName: string, relative_path: string) =>
    [...workflowKeys.all, "getLogs", projectId, branchName, relative_path] as const,
  getRuns: (
    projectId: string,
    branchName: string,
    relative_path: string,
    pagination: PaginationState
  ) => [...workflowKeys.all, "getRuns", projectId, branchName, relative_path, pagination] as const,
  getBlocks: (projectId: string, branchName: string, sourceId: string, runIndex?: number) =>
    [...workflowKeys.all, "getBlocks", projectId, branchName, sourceId, runIndex] as const
};

const chartKeys = {
  all: ["chart"] as const,
  get: (projectId: string, branchName: string, file_path: string) =>
    [...chartKeys.all, "get", projectId, branchName, file_path] as const
};

const fileKeys = {
  all: (projectId: string, branchName: string) => ["all", projectId, branchName],
  get: (projectId: string, branchName: string, pathb64: string) =>
    [...fileKeys.all(projectId, branchName), "get", pathb64] as const,
  getGit: (projectId: string, branchName: string, pathb64: string, commit: string) =>
    [...fileKeys.all(projectId, branchName), "getGit", pathb64, commit] as const,
  tree: (projectId: string, branchName: string) =>
    [...fileKeys.all(projectId, branchName), "tree"] as const,
  diffSummary: (projectId: string, branchName: string) =>
    [...fileKeys.all(projectId, branchName), "diffSummary"] as const
};

const databaseKeys = {
  all: ["database"] as const,
  list: (projectId: string, branchName: string) =>
    [...databaseKeys.all, "list", projectId, branchName] as const
};

const appKeys = {
  all: ["app"] as const,
  list: (projectId: string, branchName: string) =>
    [...appKeys.all, "list", projectId, branchName] as const,
  getAppData: (projectId: string, branchName: string, appPath: string) =>
    [...appKeys.all, "getAppData", projectId, branchName, appPath] as const,
  getData: (projectId: string, branchName: string, appPath: string) =>
    [...appKeys.all, "getData", projectId, branchName, appPath] as const,
  getDisplays: (projectId: string, branchName: string, appPath: string) =>
    [...appKeys.all, "getDisplays", projectId, branchName, appPath] as const
};

const apiKeyKeys = {
  all: ["apiKey"] as const,
  list: (projectId: string) => [...apiKeyKeys.all, "list", projectId] as const,
  item: (projectId: string, id: string) => [...apiKeyKeys.all, projectId, { id }] as const
};

const secretKeys = {
  all: ["secret"] as const,
  list: (projectId: string) => [...secretKeys.all, "list", projectId] as const,
  item: (projectId: string, id: string) => [...secretKeys.all, projectId, { id }] as const,
  envList: (projectId: string) => [...secretKeys.all, "env", projectId] as const
};

const logsKeys = {
  all: ["logs"] as const,
  list: (projectId: string) => [...logsKeys.all, "list", projectId] as const
};

const settingsKeys = {
  all: ["settings"] as const,
  revisionInfo: () => [...settingsKeys.all, "revision-info"] as const,
  projectStatus: (project_id: string) =>
    [...settingsKeys.all, "project-status", { project_id }] as const,
  currentProject: () => [...settingsKeys.all, "current-project"] as const
};

const repositoryKeys = {
  all: ["repositories"] as const,
  list: (projectId: string) => [...repositoryKeys.all, "list", projectId] as const,
  branch: (projectId: string, name: string) =>
    [...repositoryKeys.all, "branch", projectId, name] as const,
  diff: (projectId: string, name: string) =>
    [...repositoryKeys.all, "diff", projectId, name] as const,
  branches: (projectId: string, name: string) =>
    [...repositoryKeys.all, "branches", projectId, name] as const
};

const configKeys = {
  all: ["config"] as const,
  validation: () => [...configKeys.all, "validation"] as const,
  status: () => [...configKeys.all, "status"] as const
};

const userKeys = {
  all: ["user"] as const,
  list: () => [...userKeys.all, "list"] as const,
  current: () => [...userKeys.all, "current"] as const
};

const workspaceKeys = {
  all: ["workspace"] as const,
  list: () => [...workspaceKeys.all, "list"] as const,
  item: (workspaceId: string) => [...workspaceKeys.all, "item", workspaceId] as const,
  branches: (workspaceId: string) => [...workspaceKeys.all, "branches", workspaceId] as const,

  revisionInfo: (workspaceId: string, branchName: string) =>
    [...workspaceKeys.all, "revisionInfo", workspaceId, branchName] as const,

  status: (workspaceId: string, branchName: string) =>
    [...workspaceKeys.all, "status", workspaceId, branchName] as const
};

const artifactKeys = {
  all: ["artifact"] as const,
  get: (projectId: string, branchName: string, id: string) =>
    [...artifactKeys.all, "get", projectId, branchName, id] as const
};

const contextGraphKeys = {
  all: ["context-graph"] as const,
  graph: (projectId: string, branchName: string) =>
    [...contextGraphKeys.all, "graph", projectId, branchName] as const
};

const integrationKeys = {
  all: ["integration"] as const,
  looker: (projectId: string, branchName: string) =>
    [...integrationKeys.all, "looker", projectId, branchName] as const
};

const testFileKeys = {
  all: ["testFile"] as const,
  list: (projectId: string, branchName: string) =>
    [...testFileKeys.all, "list", projectId, branchName] as const,
  get: (pathb64: string, projectId: string, branchName: string) =>
    [...testFileKeys.all, "get", pathb64, projectId, branchName] as const
};

const testProjectRunKeys = {
  all: ["testProjectRun"] as const,
  list: (projectId: string) => [...testProjectRunKeys.all, "list", projectId] as const
};

const testRunKeys = {
  all: ["testRun"] as const,
  list: (projectId: string, pathb64: string) =>
    [...testRunKeys.all, "list", projectId, pathb64] as const,
  detail: (projectId: string, pathb64: string, runIndex: number) =>
    [...testRunKeys.all, "detail", projectId, pathb64, runIndex] as const
};

const humanVerdictKeys = {
  all: ["humanVerdict"] as const,
  list: (projectId: string, pathb64: string, runIndex: number) =>
    [...humanVerdictKeys.all, "list", projectId, pathb64, runIndex] as const
};

const queryKeys = {
  agent: agentKeys,
  analytics: analyticsKeys,
  thread: threadKeys,
  apiKey: apiKeyKeys,
  secret: secretKeys,
  logs: logsKeys,
  user: userKeys,
  workspaces: workspaceKeys,
  workflow: workflowKeys,
  chart: chartKeys,
  file: fileKeys,
  database: databaseKeys,
  app: appKeys,
  settings: settingsKeys,
  repositories: repositoryKeys,
  config: configKeys,
  artifact: artifactKeys,
  contextGraph: contextGraphKeys,
  integration: integrationKeys,
  trace: traceKeys,
  testFile: testFileKeys,
  testProjectRun: testProjectRunKeys,
  testRun: testRunKeys,
  humanVerdict: humanVerdictKeys
};

export default queryKeys;
