import type {
  GitHubRepository,
  ListRepositoriesResponse,
  SelectRepositoryRequest,
  SelectRepositoryResponse,
  StoreTokenRequest,
  TokenResponse
} from "@/types/github";
import type { GitHubSettings, RevisionInfo } from "@/types/settings";
import { apiClient } from "./api/axios";

export const GitHubService = {
  async storeToken(token: string): Promise<TokenResponse> {
    const response = await apiClient.post<TokenResponse>("/github/token", {
      token
    } as StoreTokenRequest);
    return response.data;
  },

  async listRepositories(): Promise<GitHubRepository[]> {
    const response = await apiClient.get<ListRepositoriesResponse>("/github/repositories");
    return response.data.repositories;
  },

  async selectRepository(repositoryId: number): Promise<SelectRepositoryResponse> {
    const response = await apiClient.post<SelectRepositoryResponse>("/github/repositories/select", {
      repository_id: repositoryId
    } as SelectRepositoryRequest);
    return response.data;
  },

  async getGithubSettings(): Promise<GitHubSettings> {
    const response = await apiClient.get("/github/settings");
    return response.data;
  },

  async getGithubRevisionInfo(): Promise<RevisionInfo> {
    const response = await apiClient.get("/github/revision");
    return response.data;
  },

  async updateGitHubToken(token: string): Promise<{ success: boolean }> {
    const response = await apiClient.put("/github/settings", { token });
    return response.data;
  },

  async updateProjectGithubApp(
    projectId: string,
    installationId: string,
    appId?: string
  ): Promise<{ success: boolean; message: string }> {
    const response = await apiClient.put(`/github/app/project/${projectId}`, {
      installation_id: installationId,
      app_id: appId
    });
    return response.data;
  },

  async syncGitHubRepository(): Promise<{ success: boolean; message: string }> {
    const response = await apiClient.post("/github/sync");
    return response.data;
  }
};
