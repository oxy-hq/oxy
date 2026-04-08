import type { FileStatus, FileTreeModel } from "@/types/file";
import type {
  AddRepositoryFromGitHubRequest,
  AddRepositoryRequest,
  Repository
} from "@/types/repository";
import { apiClient } from "./axios";

export interface RepoBranchResponse {
  branch: string;
}

export interface RepoBranchesResponse {
  branches: string[];
}

export interface RepoCommitResponse {
  success: boolean;
  message: string;
}

export class RepositoryService {
  static async listRepositories(projectId: string): Promise<Repository[]> {
    const response = await apiClient.get(`/${projectId}/repositories`);
    return response.data;
  }

  static async addRepository(
    projectId: string,
    request: AddRepositoryRequest
  ): Promise<Repository> {
    const response = await apiClient.post(`/${projectId}/repositories`, request);
    return response.data;
  }

  static async removeRepository(projectId: string, name: string): Promise<void> {
    await apiClient.delete(`/${projectId}/repositories/${encodeURIComponent(name)}`);
  }

  static async getRepoBranch(projectId: string, name: string): Promise<RepoBranchResponse> {
    const response = await apiClient.get(
      `/${projectId}/repositories/${encodeURIComponent(name)}/branch`
    );
    return response.data;
  }

  static async getRepoDiff(projectId: string, name: string): Promise<FileStatus[]> {
    const response = await apiClient.get(
      `/${projectId}/repositories/${encodeURIComponent(name)}/diff`
    );
    return response.data;
  }

  static async commitRepo(
    projectId: string,
    name: string,
    message: string
  ): Promise<RepoCommitResponse> {
    const response = await apiClient.post(
      `/${projectId}/repositories/${encodeURIComponent(name)}/commit`,
      { message }
    );
    return response.data;
  }

  static async addRepositoryFromGitHub(
    projectId: string,
    request: AddRepositoryFromGitHubRequest
  ): Promise<Repository> {
    const response = await apiClient.post(`/${projectId}/repositories/github`, request);
    return response.data;
  }

  static async getRepoFileTree(projectId: string, name: string): Promise<FileTreeModel[]> {
    const response = await apiClient.get(
      `/${projectId}/repositories/${encodeURIComponent(name)}/files`
    );
    return response.data;
  }

  static async listRepoBranches(projectId: string, name: string): Promise<RepoBranchesResponse> {
    const response = await apiClient.get(
      `/${projectId}/repositories/${encodeURIComponent(name)}/branches`
    );
    return response.data;
  }

  static async checkoutRepoBranch(projectId: string, name: string, branch: string): Promise<void> {
    await apiClient.post(`/${projectId}/repositories/${encodeURIComponent(name)}/checkout`, {
      branch
    });
  }
}
