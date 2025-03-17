const conversationKeys = {
  all: ["conversation"] as const,
  list: () => [...conversationKeys.all, "list"] as const,
  messages: (agentName: string | undefined) =>
    [...conversationKeys.all, "messages", { agentName }] as const,
};

const agentKeys = {
  all: ["agent"] as const,
  list: () => [...agentKeys.all, "list"] as const,
};

const threadKeys = {
  all: ["thread"] as const,
  list: () => [...threadKeys.all, "list"] as const,
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

const queryKeys = {
  conversation: conversationKeys,
  agent: agentKeys,
  thread: threadKeys,
  workflow: workflowKeys,
};

export default queryKeys;
