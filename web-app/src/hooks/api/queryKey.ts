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
  get: (appPath: string) => [...appKeys.all, "get", appPath] as const,
  getData: (appPath: string) => [...appKeys.all, "getData", appPath] as const,
};

const apiKeyKeys = {
  all: ["apiKey"] as const,
  list: () => [...apiKeyKeys.all, "list"] as const,
  item: (id: string) => [...apiKeyKeys.all, { id }] as const,
};

const queryKeys = {
  conversation: conversationKeys,
  agent: agentKeys,
  thread: threadKeys,
  apiKey: apiKeyKeys,
  workflow: workflowKeys,
  chart: chartKeys,
  file: fileKeys,
  database: databaseKeys,
  app: appKeys,
};

export default queryKeys;
