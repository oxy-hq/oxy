const conversationKeys = {
  all: ["conversation"] as const,
  list: () => [...conversationKeys.all, "list"] as const,
  messages: (agentName: string | undefined) =>
    [...conversationKeys.all, "messages", { agentName }] as const,
};

const agentKeys = {
  all: ["agent"] as const,
  list: () => [...agentKeys.all, "list"] as const,
  get: (pathb64: string) => [...agentKeys.all, "get", pathb64] as const,
};

const threadKeys = {
  all: ["thread"] as const,
  list: (page?: number, limit?: number) =>
    [...threadKeys.all, "list", { page, limit }] as const,
  item: (threadId: string) => [...threadKeys.all, { threadId }] as const,
};
const workflowKeys = {
  all: ["workflow"] as const,
  run: () => [...workflowKeys.all, "run"] as const,
  list: () => [...workflowKeys.all, "list"] as const,
  get: (relative_path: string) =>
    [...workflowKeys.all, "get", relative_path] as const,
  getLogs: (relative_path: string) =>
    [...workflowKeys.all, "getLogs", relative_path] as const,
};

const chartKeys = {
  all: ["chart"] as const,
  get: (file_path: string) => [...chartKeys.all, "get", file_path] as const,
};

const fileKeys = {
  all: ["file"] as const,
  get: (pathb64: string) => [...fileKeys.all, "get", pathb64] as const,
};

const databaseKeys = {
  all: ["database"] as const,
  list: () => [...databaseKeys.all, "list"] as const,
};

const appKeys = {
  all: ["app"] as const,
  list: () => [...appKeys.all, "list"] as const,
  getAppData: (appPath: string) =>
    [...appKeys.all, "getAppData", appPath] as const,
  getData: (appPath: string) => [...appKeys.all, "getData", appPath] as const,
  getDisplays: (appPath: string) =>
    [...appKeys.all, "getDisplays", appPath] as const,
};

const apiKeyKeys = {
  all: ["apiKey"] as const,
  list: () => [...apiKeyKeys.all, "list"] as const,
  item: (id: string) => [...apiKeyKeys.all, { id }] as const,
};

const secretKeys = {
  all: ["secret"] as const,
  list: () => [...secretKeys.all, "list"] as const,
  item: (id: string) => [...secretKeys.all, { id }] as const,
};

const settingsKeys = {
  all: ["settings"] as const,
  revisionInfo: () => [...settingsKeys.all, "revision-info"] as const,
  projectStatus: () => [...settingsKeys.all, "project-status"] as const,
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
  list: () => [...userKeys.all, "list"] as const,
  current: () => [...userKeys.all, "current"] as const,
};

const queryKeys = {
  conversation: conversationKeys,
  agent: agentKeys,
  thread: threadKeys,
  apiKey: apiKeyKeys,
  secret: secretKeys,
  user: userKeys,
  workflow: workflowKeys,
  chart: chartKeys,
  file: fileKeys,
  database: databaseKeys,
  app: appKeys,
  settings: settingsKeys,
  repositories: repositoryKeys,
  config: configKeys,
};

export default queryKeys;
