import { apiClient } from "./axios";
import { AgentConfig } from "@/types/agent";
import { TestStreamMessage } from "@/types/eval";
import { Answer } from "@/types/chat";
import fetchSSE from "./fetchSSE";
import { apiBaseURL } from "../env";

export class AgentService {
  static async listAgents(
    projectId: string,
    branchName: string,
  ): Promise<AgentConfig[]> {
    const response = await apiClient.get(`/${projectId}/agents`, {
      params: { branch: branchName },
    });
    return response.data;
  }

  static async getAgent(
    projectId: string,
    branchName: string,
    pathb64: string,
  ): Promise<AgentConfig> {
    const response = await apiClient.get(
      `/${projectId}/agents/${encodeURIComponent(pathb64)}`,
      {
        params: { branch: branchName },
      },
    );
    return response.data;
  }

  static async runTestAgent(
    projectId: string,
    branchName: string,
    pathb64: string,
    testIndex: number,
    onReadStream: (event: TestStreamMessage) => void,
  ): Promise<void> {
    const searchParams = new URLSearchParams({
      branch: branchName,
    });
    const url = `${apiBaseURL}/${projectId}/agents/${encodeURIComponent(pathb64)}/tests/${testIndex}?${searchParams.toString()}`;
    await fetchSSE(url, {
      onMessage: onReadStream,
    });
  }

  static async askAgentPreview(
    projectId: string,
    branchName: string,
    agentPathb64: string,
    question: string,
    onReadStream: (answer: Answer) => void,
  ): Promise<void> {
    const searchParams = new URLSearchParams({
      branch: branchName,
    });
    const url = `${apiBaseURL}/${projectId}/agents/${encodeURIComponent(agentPathb64)}/ask?${searchParams.toString()}`;
    await fetchSSE(url, {
      body: { question },
      onMessage: onReadStream,
    });
  }
}
