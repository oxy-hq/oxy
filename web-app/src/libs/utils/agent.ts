export const getAgentNameFromPath = (agentPath: string) =>
  agentPath.split("/").pop()?.replace(".agent.yml", "") ?? "";
