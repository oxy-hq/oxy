import { apiClient } from "./axios";
import { GitHubRepository, GitHubBranch } from "@/types/github";

export class GitHubApiService {
  static async listRepositories(token: string): Promise<GitHubRepository[]> {
    const response = await apiClient.get("/github/repositories", {
      params: { token },
    });
    return response.data;
  }

  static async listBranches(
    token: string,
    repo: string,
  ): Promise<GitHubBranch[]> {
    const response = await apiClient.get("/github/branches", {
      params: { token, repo_full_name: repo },
    });
    return response.data;
  }
}
