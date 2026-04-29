import type {
  AnalyzeOutput,
  ColumnLineageOutput,
  CompileOutput,
  LineageOutput,
  ModelingProjectInfo,
  NodeSummary,
  RunOutput,
  RunRequest,
  RunStreamEvent,
  SeedOutput,
  TestOutput
} from "@/types/modeling";
import { apiBaseURL } from "../env";
import { apiClient } from "./axios";
import fetchSSE from "./fetchSSE";

export class ModelingService {
  static async listProjects(projectId: string, branchName: string): Promise<ModelingProjectInfo[]> {
    const response = await apiClient.get(`/${projectId}/modeling`, {
      params: { branch: branchName }
    });
    return response.data;
  }

  static async getProjectInfo(
    projectId: string,
    modelingProjectName: string,
    branchName: string
  ): Promise<ModelingProjectInfo> {
    const response = await apiClient.get(`/${projectId}/modeling/${modelingProjectName}`, {
      params: { branch: branchName }
    });
    return response.data;
  }

  static async listNodes(
    projectId: string,
    modelingProjectName: string,
    branchName: string
  ): Promise<NodeSummary[]> {
    const response = await apiClient.get(`/${projectId}/modeling/${modelingProjectName}/nodes`, {
      params: { branch: branchName }
    });
    return response.data;
  }

  static async compileProject(
    projectId: string,
    modelingProjectName: string,
    branchName: string
  ): Promise<CompileOutput> {
    const response = await apiClient.post(
      `/${projectId}/modeling/${modelingProjectName}/compile`,
      null,
      { params: { branch: branchName } }
    );
    return response.data;
  }

  static async compileModel(
    projectId: string,
    modelingProjectName: string,
    modelName: string,
    branchName: string
  ): Promise<string> {
    const response = await apiClient.post(
      `/${projectId}/modeling/${modelingProjectName}/compile/${modelName}`,
      null,
      { params: { branch: branchName } }
    );
    return response.data;
  }

  static async runModels(
    projectId: string,
    modelingProjectName: string,
    request: RunRequest,
    branchName: string
  ): Promise<RunOutput> {
    const response = await apiClient.post(
      `/${projectId}/modeling/${modelingProjectName}/run`,
      request,
      { params: { branch: branchName } }
    );
    return response.data;
  }

  static async runTests(
    projectId: string,
    modelingProjectName: string,
    request: RunRequest,
    branchName: string
  ): Promise<TestOutput> {
    const response = await apiClient.post(
      `/${projectId}/modeling/${modelingProjectName}/test`,
      request,
      { params: { branch: branchName } }
    );
    return response.data;
  }

  static async analyzeProject(
    projectId: string,
    modelingProjectName: string,
    branchName: string
  ): Promise<AnalyzeOutput> {
    const response = await apiClient.post(
      `/${projectId}/modeling/${modelingProjectName}/analyze`,
      null,
      { params: { branch: branchName } }
    );
    return response.data;
  }

  static async getLineage(
    projectId: string,
    modelingProjectName: string,
    branchName: string
  ): Promise<LineageOutput> {
    const response = await apiClient.get(`/${projectId}/modeling/${modelingProjectName}/lineage`, {
      params: { branch: branchName }
    });
    return response.data;
  }

  static async runModelsStream(
    projectId: string,
    modelingProjectName: string,
    request: RunRequest,
    branchName: string,
    onEvent: (event: RunStreamEvent) => void,
    signal?: AbortSignal
  ): Promise<void> {
    const url = `${apiBaseURL}/${projectId}/modeling/${modelingProjectName}/run/stream?branch=${encodeURIComponent(branchName)}`;
    await fetchSSE<RunStreamEvent>(url, {
      body: request,
      onMessage: onEvent,
      signal
    });
  }

  static async seedProject(
    projectId: string,
    modelingProjectName: string,
    branchName: string
  ): Promise<SeedOutput> {
    const response = await apiClient.post(
      `/${projectId}/modeling/${modelingProjectName}/seed`,
      null,
      { params: { branch: branchName } }
    );
    return response.data;
  }

  static async getColumnLineage(
    projectId: string,
    modelingProjectName: string,
    branchName: string
  ): Promise<ColumnLineageOutput> {
    const response = await apiClient.get(
      `/${projectId}/modeling/${modelingProjectName}/lineage/columns`,
      { params: { branch: branchName } }
    );
    return response.data;
  }
}
