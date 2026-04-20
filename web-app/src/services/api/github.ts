import type {
  GitHubAccount,
  GitHubBranch,
  GitHubCallbackBody,
  GitHubCallbackResponse,
  GitHubNamespace,
  GitHubRepository,
  UserInstallation
} from "@/types/github";
import { apiClient } from "./axios";

export class GitHubApiService {
  // Git Namespaces
  static async listGitNamespaces(orgId: string): Promise<GitHubNamespace[]> {
    const response = await apiClient.get(`/orgs/${orgId}/github/namespaces`);
    return response.data.installations;
  }

  static async createPATNamespace(orgId: string, token: string): Promise<GitHubNamespace> {
    const response = await apiClient.post(`/orgs/${orgId}/github/namespaces/pat`, { token });
    return response.data;
  }

  static async createInstallationNamespace(
    orgId: string,
    installationId: number
  ): Promise<GitHubNamespace> {
    const response = await apiClient.post(`/orgs/${orgId}/github/namespaces/installation`, {
      installation_id: installationId
    });
    return response.data;
  }

  static async deleteGitNamespace(orgId: string, id: string): Promise<void> {
    await apiClient.delete(`/orgs/${orgId}/github/namespaces/${id}`);
  }

  // Repositories and Branches
  static async listRepositories(
    orgId: string,
    gitNamespaceId: string
  ): Promise<GitHubRepository[]> {
    const response = await apiClient.get(`/orgs/${orgId}/github/repositories`, {
      params: { git_namespace_id: gitNamespaceId }
    });
    return response.data;
  }

  static async listBranches(
    orgId: string,
    gitNamespaceId: string,
    repoName: string
  ): Promise<GitHubBranch[]> {
    const response = await apiClient.get(`/orgs/${orgId}/github/branches`, {
      params: { git_namespace_id: gitNamespaceId, repo_name: repoName }
    });
    return response.data;
  }

  // User-scoped GitHub account and installations
  static async getAccount(): Promise<GitHubAccount> {
    const response = await apiClient.get<GitHubAccount>("/user/github/account");
    return response.data;
  }

  static async deleteAccount(): Promise<void> {
    await apiClient.delete("/user/github/account");
  }

  static async getOauthUrl(orgId: string, origin: string): Promise<string> {
    const response = await apiClient.get<{ url: string }>("/user/github/account/oauth-url", {
      params: { org_id: orgId, origin }
    });
    return response.data.url;
  }

  static async getNewInstallationUrl(orgId: string, origin: string): Promise<string> {
    const response = await apiClient.get<{ url: string }>("/user/github/installations/new-url", {
      params: { org_id: orgId, origin }
    });
    return response.data.url;
  }

  static async completeCallback(body: GitHubCallbackBody): Promise<GitHubCallbackResponse> {
    const response = await apiClient.post<GitHubCallbackResponse>("/user/github/callback", body);
    return response.data;
  }

  static async listUserInstallations(): Promise<UserInstallation[]> {
    const response = await apiClient.get<UserInstallation[]>("/user/github/installations");
    return response.data;
  }
}
