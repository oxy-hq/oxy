import { PaginationState } from "@tanstack/react-table";

const agentKeys = {
  all: ["agent"] as const,
  list: (projectId: string, branchName: string) =>
    [...agentKeys.all, "list", projectId, branchName] as const,
  get: (pathb64: string, projectId: string, branchName: string) =>
    [...agentKeys.all, "get", pathb64, projectId, branchName] as const,
};

const threadKeys = {
  all: ["thread"] as const,
  list: (projectId: string, page?: number, limit?: number) =>
    [...threadKeys.all, "list", projectId, { page, limit }] as const,
  item: (projectId: string, threadId: string) =>
    [...threadKeys.all, projectId, { threadId }] as const,
  messages: (projectId: string, threadId: string) =>
    [...threadKeys.all, "messages", projectId, threadId] as const,
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
    [
      ...workflowKeys.all,
      "getLogs",
      projectId,
      branchName,
      relative_path,
    ] as const,
  getRuns: (
    projectId: string,
    branchName: string,
    relative_path: string,
    pagination: PaginationState,
  ) =>
    [
      ...workflowKeys.all,
      "getRuns",
      projectId,
      branchName,
      relative_path,
      pagination,
    ] as const,
  getBlocks: (
    projectId: string,
    branchName: string,
    sourceId: string,
    runIndex?: number,
  ) =>
    [
      ...workflowKeys.all,
      "getBlocks",
      projectId,
      branchName,
      sourceId,
      runIndex,
    ] as const,
};

const chartKeys = {
  all: ["chart"] as const,
  get: (projectId: string, branchName: string, file_path: string) =>
    [...chartKeys.all, "get", projectId, branchName, file_path] as const,
};

const fileKeys = {
  all: (projectId: string, branchName: string) => [
    "all",
    projectId,
    branchName,
  ],
  get: (projectId: string, branchName: string, pathb64: string) =>
    [...fileKeys.all(projectId, branchName), "get", pathb64] as const,
  getGit: (
    projectId: string,
    branchName: string,
    pathb64: string,
    commit: string,
  ) =>
    [
      ...fileKeys.all(projectId, branchName),
      "getGit",
      pathb64,
      commit,
    ] as const,
  tree: (projectId: string, branchName: string) =>
    [...fileKeys.all(projectId, branchName), "tree"] as const,
  diffSummary: (projectId: string, branchName: string) =>
    [...fileKeys.all(projectId, branchName), "diffSummary"] as const,
};

const databaseKeys = {
  all: ["database"] as const,
  list: (projectId: string, branchName: string) =>
    [...databaseKeys.all, "list", projectId, branchName] as const,
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
    [...appKeys.all, "getDisplays", projectId, branchName, appPath] as const,
};

const apiKeyKeys = {
  all: ["apiKey"] as const,
  list: (projectId: string) => [...apiKeyKeys.all, "list", projectId] as const,
  item: (projectId: string, id: string) =>
    [...apiKeyKeys.all, projectId, { id }] as const,
};

const secretKeys = {
  all: ["secret"] as const,
  list: (projectId: string) => [...secretKeys.all, "list", projectId] as const,
  item: (projectId: string, id: string) =>
    [...secretKeys.all, projectId, { id }] as const,
};

const logsKeys = {
  all: ["logs"] as const,
  list: (projectId: string) => [...logsKeys.all, "list", projectId] as const,
};

const settingsKeys = {
  all: ["settings"] as const,
  revisionInfo: () => [...settingsKeys.all, "revision-info"] as const,
  projectStatus: (project_id: string) =>
    [...settingsKeys.all, "project-status", { project_id }] as const,
  currentProject: () => [...settingsKeys.all, "current-project"] as const,
};

const repositoryKeys = {
  all: ["repositories"] as const,
};

const configKeys = {
  all: ["config"] as const,
  validation: () => [...configKeys.all, "validation"] as const,
  status: () => [...configKeys.all, "status"] as const,
};

const userKeys = {
  all: ["user"] as const,
  list: (organizationId: string) =>
    [...userKeys.all, "list", organizationId] as const,
  current: () => [...userKeys.all, "current"] as const,
};

const organizationKeys = {
  all: ["organization"] as const,
  list: () => [...organizationKeys.all, "list"] as const,
  item: (id: string) => [...organizationKeys.all, { id }] as const,
};

const projectKeys = {
  all: ["project"] as const,
  list: (organizationId: string) =>
    [...projectKeys.all, "list", organizationId] as const,
  item: (projectId: string) => [...projectKeys.all, "item", projectId] as const,
  branches: (projectId: string) =>
    [...projectKeys.all, "branches", projectId] as const,

  revisionInfo: (projectId: string, branchName: string) =>
    [...projectKeys.all, "revisionInfo", projectId, branchName] as const,

  status: (projectId: string, branchName: string) =>
    [...projectKeys.all, "status", projectId, branchName] as const,
};

const artifactKeys = {
  all: ["artifact"] as const,
  get: (projectId: string, branchName: string, id: string) =>
    [...artifactKeys.all, "get", projectId, branchName, id] as const,
};

const queryKeys = {
  agent: agentKeys,
  thread: threadKeys,
  apiKey: apiKeyKeys,
  secret: secretKeys,
  logs: logsKeys,
  user: userKeys,
  organizations: organizationKeys,
  projects: projectKeys,
  workflow: workflowKeys,
  chart: chartKeys,
  file: fileKeys,
  database: databaseKeys,
  app: appKeys,
  settings: settingsKeys,
  repositories: repositoryKeys,
  config: configKeys,
  artifact: artifactKeys,
};

export default queryKeys;
