import { ProjectStatus } from "@/types/github";
import { apiClient } from "./axios";
import {
  Project,
  ProjectsResponse,
  CreateProjectRequest,
  ProjectBranchesResponse,
  ProjectBranch,
  CreateProjectResponse,
} from "@/types/project";
import { RevisionInfo } from "@/types/settings";

export class ProjectService {
  static async listProjects(organizationId: string): Promise<ProjectsResponse> {
    const response = await apiClient.get(
      `/organizations/${organizationId}/projects`,
    );
    return response.data;
  }

  static async getGithubRevisionInfo(
    projectId: string,
    branchName: string,
  ): Promise<RevisionInfo> {
    const response = await apiClient.get(`/${projectId}/revision-info`, {
      params: { branch: branchName },
    });
    return response.data;
  }

  static async getProject(projectId: string): Promise<Project> {
    const response = await apiClient.get(`/${projectId}/details`);
    return response.data;
  }

  static async createProject(
    organizationId: string,
    request: CreateProjectRequest,
  ): Promise<CreateProjectResponse> {
    const response = await apiClient.post(
      `/organizations/${organizationId}/projects`,
      request,
    );
    return response.data;
  }

  static async deleteProject(
    organizationId: string,
    projectId: string,
  ): Promise<void> {
    await apiClient.delete(
      `/organizations/${organizationId}/projects/${projectId}`,
    );
  }

  static async getProjectBranches(
    projectId: string,
  ): Promise<ProjectBranchesResponse> {
    const response = await apiClient.get(`/${projectId}/branches`);
    return response.data;
  }

  static async getProjectStatus(
    project_id: string,
    branch_name?: string,
  ): Promise<ProjectStatus> {
    const response = await apiClient.get<ProjectStatus>(
      `/${project_id}/status`,
      branch_name
        ? {
            params: { branch: branch_name },
          }
        : undefined,
    );
    return response.data;
  }

  static async switchProjectBranch(
    projectId: string,
    branchName: string,
  ): Promise<ProjectBranch> {
    const response = await apiClient.post(`/${projectId}/switch-branch`, {
      branch: branchName,
    });
    return response.data;
  }

  static async switchProjectActiveBranch(
    projectId: string,
    branchName: string,
  ): Promise<ProjectBranch> {
    const response = await apiClient.post(
      `/${projectId}/switch-active-branch`,
      {
        branch: branchName,
      },
    );
    return response.data;
  }

  static async pullChanges(
    projectId: string,
    branchName: string,
  ): Promise<{ success: boolean; message: string }> {
    const response = await apiClient.post(`/${projectId}/pull-changes`, null, {
      params: { branch: branchName },
    });
    return response.data;
  }

  static async pushChanges(
    projectId: string,
    branchName: string,
    commitMessage?: string,
  ): Promise<{ success: boolean; message: string }> {
    const response = await apiClient.post(
      `/${projectId}/push-changes`,
      {
        commit_message: commitMessage,
      },
      {
        params: { branch: branchName },
      },
    );
    return response.data;
  }

  static async updateGitHubToken(
    token: string,
    projectId: string,
  ): Promise<{ success: boolean; message: string }> {
    const response = await apiClient.post(`/${projectId}/git-token`, { token });
    return response.data;
  }
}
