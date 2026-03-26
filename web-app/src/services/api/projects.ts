import type { ProjectStatus } from "@/types/github";
import type { Project, ProjectBranch, ProjectBranchesResponse } from "@/types/project";
import type { RevisionInfo } from "@/types/settings";
import { apiClient } from "./axios";

export interface CommitEntry {
  hash: string;
  short_hash: string;
  message: string;
  author: string;
  date: string;
}

export interface RecentCommitsResponse {
  commits: CommitEntry[];
}

export class ProjectService {
  static async getGithubRevisionInfo(projectId: string, branchName: string): Promise<RevisionInfo> {
    const response = await apiClient.get(`/${projectId}/revision-info`, {
      params: { branch: branchName }
    });
    return response.data;
  }

  static async getProject(projectId: string): Promise<Project> {
    const response = await apiClient.get(`/${projectId}/details`);
    return response.data;
  }

  static async deleteProject(workspaceId: string, projectId: string): Promise<void> {
    await apiClient.delete(`/workspaces/${workspaceId}/projects/${projectId}`);
  }

  static async getProjectBranches(projectId: string): Promise<ProjectBranchesResponse> {
    const response = await apiClient.get(`/${projectId}/branches`);
    return response.data;
  }

  static async getProjectStatus(project_id: string, branch_name?: string): Promise<ProjectStatus> {
    const response = await apiClient.get<ProjectStatus>(
      `/${project_id}/status`,
      branch_name
        ? {
            params: { branch: branch_name }
          }
        : undefined
    );
    return response.data;
  }

  static async switchProjectBranch(projectId: string, branchName: string): Promise<ProjectBranch> {
    const response = await apiClient.post(`/${projectId}/switch-branch`, {
      branch: branchName
    });
    return response.data;
  }

  static async switchProjectActiveBranch(
    projectId: string,
    branchName: string
  ): Promise<ProjectBranch> {
    const response = await apiClient.post(`/${projectId}/switch-active-branch`, {
      branch: branchName
    });
    return response.data;
  }

  static async pullChanges(
    projectId: string,
    branchName: string
  ): Promise<{ success: boolean; message: string }> {
    const response = await apiClient.post(`/${projectId}/pull-changes`, null, {
      params: { branch: branchName }
    });
    return response.data;
  }

  static async continueRebase(
    projectId: string,
    branchName: string
  ): Promise<{ success: boolean; message: string }> {
    const response = await apiClient.post(`/${projectId}/continue-rebase`, null, {
      params: { branch: branchName }
    });
    return response.data;
  }

  static async resolveConflictWithContent(
    projectId: string,
    branchName: string,
    filePath: string,
    content: string
  ): Promise<{ success: boolean; message: string }> {
    const response = await apiClient.post(
      `/${projectId}/resolve-conflict-with-content`,
      { content },
      { params: { branch: branchName, file: filePath } }
    );
    return response.data;
  }

  static async unresolveConflictFile(
    projectId: string,
    branchName: string,
    filePath: string
  ): Promise<{ success: boolean; message: string }> {
    const response = await apiClient.post(`/${projectId}/unresolve-conflict-file`, null, {
      params: { branch: branchName, file: filePath }
    });
    return response.data;
  }

  static async resolveConflictFile(
    projectId: string,
    branchName: string,
    filePath: string,
    side: "mine" | "theirs"
  ): Promise<{ success: boolean; message: string }> {
    const response = await apiClient.post(`/${projectId}/resolve-conflict-file`, null, {
      params: { branch: branchName, file: filePath, side }
    });
    return response.data;
  }

  static async abortRebase(
    projectId: string,
    branchName: string
  ): Promise<{ success: boolean; message: string }> {
    const response = await apiClient.post(`/${projectId}/abort-rebase`, null, {
      params: { branch: branchName }
    });
    return response.data;
  }

  static async pushChanges(
    projectId: string,
    branchName: string,
    commitMessage?: string
  ): Promise<{ success: boolean; message: string }> {
    const response = await apiClient.post(
      `/${projectId}/push-changes`,
      {
        commit_message: commitMessage
      },
      {
        params: { branch: branchName }
      }
    );
    return response.data;
  }

  static async updateGitHubToken(
    token: string,
    projectId: string
  ): Promise<{ success: boolean; message: string }> {
    const response = await apiClient.post(`/${projectId}/git-token`, { token });
    return response.data;
  }

  static async deleteBranch(
    projectId: string,
    branchName: string
  ): Promise<{ success: boolean; message: string }> {
    const response = await apiClient.delete(
      `/${projectId}/branches/${encodeURIComponent(branchName)}`
    );
    return response.data;
  }

  static async forcePushBranch(
    projectId: string,
    branchName: string
  ): Promise<{ success: boolean; message: string }> {
    const response = await apiClient.post(`/${projectId}/force-push`, null, {
      params: { branch: branchName }
    });
    return response.data;
  }

  static async getRecentCommits(
    projectId: string,
    branchName: string
  ): Promise<RecentCommitsResponse> {
    const response = await apiClient.get(`/${projectId}/recent-commits`, {
      params: { branch: branchName }
    });
    return response.data;
  }

  static async resetToCommit(
    projectId: string,
    branchName: string,
    commit: string
  ): Promise<{ success: boolean; message: string }> {
    const response = await apiClient.post(`/${projectId}/reset-to-commit`, null, {
      params: { branch: branchName, commit }
    });
    return response.data;
  }

  static async createRepoFromProject(
    projectId: string,
    gitNamespaceId: string,
    repoName: string
  ): Promise<{ success: boolean; message: string }> {
    const response = await apiClient.post(`/${projectId}/create-repo`, {
      git_namespace_id: gitNamespaceId,
      repo_name: repoName
    });
    return response.data;
  }
}
