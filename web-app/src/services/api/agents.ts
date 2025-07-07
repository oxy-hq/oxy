import { apiClient } from "./axios";
import { AgentConfig } from "@/types/agent";
import { TestStreamMessage } from "@/types/eval";
import { Answer } from "@/types/chat";
import fetchSSE from "./fetchSSE";
import { apiBaseURL } from "../env";

export class AgentService {
  static async listAgents(): Promise<AgentConfig[]> {
    const response = await apiClient.get("/agents");
    return response.data;
  }

  static async getAgent(pathb64: string): Promise<AgentConfig> {
    const response = await apiClient.get("/agents/" + pathb64);
    return response.data;
  }

  static async runTestAgent(
    pathb64: string,
    testIndex: number,
    onReadStream: (event: TestStreamMessage) => void,
  ): Promise<void> {
    const url = `${apiBaseURL}/agents/${pathb64}/tests/${testIndex}`;
    await fetchSSE(url, {
      onMessage: onReadStream,
    });
  }

  static async askAgentPreview(
    agentPathb64: string,
    question: string,
    onReadStream: (answer: Answer) => void,
  ): Promise<void> {
    const url = `${apiBaseURL}/agents/${agentPathb64}/ask`;
    await fetchSSE(url, {
      body: { question },
      onMessage: onReadStream,
    });
  }
}
