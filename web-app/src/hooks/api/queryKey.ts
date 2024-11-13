const conversationKeys = {
  all: ["conversation"] as const,
  list: () => [...conversationKeys.all, "list"] as const,
  messages: (agentName: string | undefined) =>
    [...conversationKeys.all, "messages", { agentName }] as const
};

const agentKeys = {
  all: ["agent"] as const,
  list: () => [...agentKeys.all, "list"] as const
};

const queryKeys = {
  conversation: conversationKeys,
  agent: agentKeys
};

export default queryKeys;

