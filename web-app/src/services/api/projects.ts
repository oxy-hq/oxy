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

export const ProjectService = {
  async getGithubRevisionInfo(projectId: string, branchName: string): Promise<RevisionInfo> {
    const response = await apiClient.get(`/${projectId}/revision-info`, {
      params: { branch: branchName }
    });
    return response.data;
  },

  async getProject(projectId: string): Promise<Project> {
    const response = await apiClient.get(`/${projectId}/details`);
    return response.data;
  },

  async getProjectBranches(projectId: string): Promise<ProjectBranchesResponse> {
    const response = await apiClient.get(`/${projectId}/branches`);
    return response.data;
  },

  async getProjectStatus(project_id: string, branch_name?: string): Promise<ProjectStatus> {
    const response = await apiClient.get<ProjectStatus>(
      `/${project_id}/status`,
      branch_name ? { params: { branch: branch_name } } : undefined
    );
    return response.data;
  },

  async switchProjectBranch(projectId: string, branchName: string): Promise<ProjectBranch> {
    const response = await apiClient.post(`/${projectId}/switch-branch`, { branch: branchName });
    return response.data;
  },

  async switchProjectActiveBranch(projectId: string, branchName: string): Promise<ProjectBranch> {
    const response = await apiClient.post(`/${projectId}/switch-active-branch`, {
      branch: branchName
    });
    return response.data;
  },

  async pullChanges(
    projectId: string,
    branchName: string
  ): Promise<{ success: boolean; message: string }> {
    const response = await apiClient.post(`/${projectId}/pull-changes`, null, {
      params: { branch: branchName }
    });
    return response.data;
  },

  async continueRebase(
    projectId: string,
    branchName: string
  ): Promise<{ success: boolean; message: string }> {
    const response = await apiClient.post(`/${projectId}/continue-rebase`, null, {
      params: { branch: branchName }
    });
    return response.data;
  },

  async resolveConflictWithContent(
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
  },

  async unresolveConflictFile(
    projectId: string,
    branchName: string,
    filePath: string
  ): Promise<{ success: boolean; message: string }> {
    const response = await apiClient.post(`/${projectId}/unresolve-conflict-file`, null, {
      params: { branch: branchName, file: filePath }
    });
    return response.data;
  },

  async resolveConflictFile(
    projectId: string,
    branchName: string,
    filePath: string,
    side: "mine" | "theirs"
  ): Promise<{ success: boolean; message: string }> {
    const response = await apiClient.post(`/${projectId}/resolve-conflict-file`, null, {
      params: { branch: branchName, file: filePath, side }
    });
    return response.data;
  },

  async abortRebase(
    projectId: string,
    branchName: string
  ): Promise<{ success: boolean; message: string }> {
    const response = await apiClient.post(`/${projectId}/abort-rebase`, null, {
      params: { branch: branchName }
    });
    return response.data;
  },

  async pushChanges(
    projectId: string,
    branchName: string,
    commitMessage?: string
  ): Promise<{ success: boolean; message: string }> {
    const response = await apiClient.post(
      `/${projectId}/push-changes`,
      { commit_message: commitMessage },
      { params: { branch: branchName } }
    );
    return response.data;
  },

  async updateGitHubToken(
    token: string,
    projectId: string
  ): Promise<{ success: boolean; message: string }> {
    const response = await apiClient.post(`/${projectId}/git-token`, { token });
    return response.data;
  },

  async deleteBranch(
    projectId: string,
    branchName: string
  ): Promise<{ success: boolean; message: string }> {
    const response = await apiClient.delete(
      `/${projectId}/branches/${encodeURIComponent(branchName)}`
    );
    return response.data;
  },

  async forcePushBranch(
    projectId: string,
    branchName: string
  ): Promise<{ success: boolean; message: string }> {
    const response = await apiClient.post(`/${projectId}/force-push`, null, {
      params: { branch: branchName }
    });
    return response.data;
  },

  async getRecentCommits(projectId: string, branchName: string): Promise<RecentCommitsResponse> {
    const response = await apiClient.get(`/${projectId}/recent-commits`, {
      params: { branch: branchName }
    });
    return response.data;
  },

  async resetToCommit(
    projectId: string,
    branchName: string,
    commit: string
  ): Promise<{ success: boolean; message: string }> {
    const response = await apiClient.post(`/${projectId}/reset-to-commit`, null, {
      params: { branch: branchName, commit }
    });
    return response.data;
  },

  async createRepoFromProject(
    projectId: string,
    gitNamespaceId: string,
    repoName: string
  ): Promise<{ success: boolean; message: string }> {
    const response = await apiClient.post(`/${projectId}/create-repo`, {
      git_namespace_id: gitNamespaceId,
      repo_name: repoName
    });
    return response.data;
  },

  async listAllProjects(): Promise<ProjectSummary[]> {
    const response = await apiClient.get("/projects");
    return response.data;
  },

  async deleteProject(projectId: string, deleteFiles = false): Promise<void> {
    await apiClient.delete(`/projects/${projectId}`, {
      params: { delete_files: deleteFiles }
    });
  },

  async activateProject(projectId: string): Promise<void> {
    await apiClient.post(`/projects/${projectId}/activate`);
  }
};

export interface ProjectSummary {
  id: string;
  name: string;
  path: string | null;
  created_at: string;
  last_opened_at: string | null;
  active: boolean;
  created_by_name: string | null;
  is_cloning: boolean;
  agent_count: number;
  workflow_count: number;
  app_count: number;
  git_remote: string | null;
  git_commit: string | null;
  git_updated_at: string | null;
}
