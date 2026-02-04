import type {
  CreateGitNamespaceRequest,
  GitHubBranch,
  GitHubNamespace,
  GitHubRepository
} from "@/types/github";
import { apiClient } from "./axios";

export class GitHubApiService {
  // Git Namespaces
  static async listGitNamespaces(): Promise<GitHubNamespace[]> {
    const response = await apiClient.get("/github/namespaces");
    return response.data.installations;
  }

  static async getInstallAppUrl(): Promise<string> {
    const response = await apiClient.get("/github/install-app-url");
    return response.data;
  }

  static async createGitNamespace(data: CreateGitNamespaceRequest): Promise<GitHubNamespace> {
    const response = await apiClient.post("/github/namespaces", data);
    return response.data;
  }

  // Repositories and Branches
  static async listRepositories(gitNamespaceId: string): Promise<GitHubRepository[]> {
    const response = await apiClient.get("/github/repositories", {
      params: { git_namespace_id: gitNamespaceId }
    });
    return response.data;
  }

  static async listBranches(gitNamespaceId: string, repoName: string): Promise<GitHubBranch[]> {
    const response = await apiClient.get("/github/branches", {
      params: { git_namespace_id: gitNamespaceId, repo_name: repoName }
    });
    return response.data;
  }
}
