import { apiClient } from "./axios";

/// Lightweight listing entry returned by `GET /{workspaceId}/agents`.
///
/// The classic `.agent.yml` execution surface has been removed; this
/// endpoint now only returns agentic (`.agentic.yml`) + analytics-workflow
/// (`.aw.yml`) agents — the chat panel's selector is the only consumer.
export type AgentInfo = {
  /// Display name (parsed from the file or `llm.ref` snippet).
  name: string;
  /// Workspace-relative path to the agent file.
  path: string;
  /// Whether the agent is exposed publicly. Always `true` for agentic agents.
  public: boolean;
  /// Model ref this agent resolves through.
  model?: string;
};

export const AgentService = {
  async listAgents(projectId: string, branchName?: string): Promise<AgentInfo[]> {
    const params: Record<string, string> = {};
    if (branchName) params.branch = branchName;
    const res = await apiClient.get<AgentInfo[]>(`/${projectId}/agents`, { params });
    return res.data;
  }
};
