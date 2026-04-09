import type {
  CreateGitNamespaceRequest,
  GitHubBranch,
  GitHubNamespace,
  GitHubRepository,
  OAuthConnectResponse
} from "@/types/github";
import { apiClient } from "./axios";

export class GitHubApiService {
  // Git Namespaces
  static async listGitNamespaces(): Promise<GitHubNamespace[]> {
    const response = await apiClient.get("/github/namespaces");
    return response.data.installations;
  }

  static async getInstallAppUrl(): Promise<string> {
    const response = await apiClient.get("/github/install-app-url", {
      params: { origin: window.location.origin }
    });
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

  static async createPATNamespace(token: string): Promise<GitHubNamespace> {
    const response = await apiClient.post("/github/namespaces/pat", { token });
    return response.data;
  }

  static async createInstallationNamespace(installationId: number): Promise<GitHubNamespace> {
    const response = await apiClient.post("/github/namespaces/installation", {
      installation_id: installationId
    });
    return response.data;
  }

  static async deleteGitNamespace(id: string): Promise<void> {
    await apiClient.delete(`/github/namespaces/${id}`);
  }

  static async getOAuthConnectUrl(): Promise<string> {
    const response = await apiClient.get<string>("/github/oauth-connect-url", {
      params: { origin: window.location.origin }
    });
    return response.data;
  }

  static async connectNamespaceFromOAuth(
    code: string,
    state: string
  ): Promise<OAuthConnectResponse> {
    const response = await apiClient.post<OAuthConnectResponse>("/github/namespaces/oauth", {
      code,
      state,
      origin: window.location.origin
    });
    return response.data;
  }

  static async pickNamespaceInstallation(
    installation_id: number,
    selection_token: string
  ): Promise<GitHubNamespace> {
    const response = await apiClient.post<GitHubNamespace>("/github/namespaces/pick", {
      installation_id,
      selection_token
    });
    return response.data;
  }
}
