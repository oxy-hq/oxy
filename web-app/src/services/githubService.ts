import { apiClient } from "./api/axios";
import {
  GitHubRepository,
  CurrentProject,
  StoreTokenRequest,
  TokenResponse,
  SelectRepositoryRequest,
  SelectRepositoryResponse,
  ListRepositoriesResponse,
} from "@/types/github";
import { RevisionInfo, GitHubSettings } from "@/types/settings";

export class GitHubService {
  /**
   * Store and validate GitHub token
   */
  static async storeToken(token: string): Promise<TokenResponse> {
    const response = await apiClient.post<TokenResponse>("/github/token", {
      token,
    } as StoreTokenRequest);
    return response.data;
  }

  /**
   * List accessible GitHub repositories
   */
  static async listRepositories(): Promise<GitHubRepository[]> {
    const response = await apiClient.get<ListRepositoriesResponse>(
      "/github/repositories",
    );
    return response.data.repositories;
  }

  /**
   * Select a repository
   */
  static async selectRepository(
    repositoryId: number,
  ): Promise<SelectRepositoryResponse> {
    const response = await apiClient.post<SelectRepositoryResponse>(
      "/github/repositories/select",
      {
        repository_id: repositoryId,
      } as SelectRepositoryRequest,
    );
    return response.data;
  }

  /**
   * Get current project information
   */
  static async getCurrentProject(): Promise<CurrentProject> {
    const response = await apiClient.get<CurrentProject>("/projects/current");
    return response.data;
  }

  /**
   * Pull latest changes for current repository
   */
  static async pullRepository(): Promise<TokenResponse> {
    const response = await apiClient.post<TokenResponse>("/git/pull");
    return response.data;
  }

  /**
   * Get GitHub settings
   */
  static async getGithubSettings(): Promise<GitHubSettings> {
    const response = await apiClient.get("/github/settings");
    return response.data;
  }

  /**
   * Get GitHub revision information
   */
  static async getGithubRevisionInfo(): Promise<RevisionInfo> {
    const response = await apiClient.get("/github/revision");
    return response.data;
  }

  /**
   * Update GitHub token
   */
  static async updateGitHubToken(token: string): Promise<{ success: boolean }> {
    const response = await apiClient.put("/github/settings", { token });
    return response.data;
  }

  /**
   * Sync GitHub repository
   */
  static async syncGitHubRepository(): Promise<{
    success: boolean;
    message: string;
  }> {
    const response = await apiClient.post("/github/sync");
    return response.data;
  }

  /**
   * Set onboarded status
   */
  static async setOnboarded(
    onboarded: boolean,
  ): Promise<{ success: boolean; message: string }> {
    const response = await apiClient.put("/github/onboarded", { onboarded });
    return response.data;
  }
}
