import type { ProjectStatus } from "@/types/github";
import type { RevisionInfo } from "@/types/settings";
import type { Workspace, WorkspaceBranch, WorkspaceBranchesResponse } from "@/types/workspace";
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

export const WorkspaceService = {
  async getGithubRevisionInfo(workspaceId: string, branchName: string): Promise<RevisionInfo> {
    const response = await apiClient.get(`/${workspaceId}/revision-info`, {
      params: { branch: branchName }
    });
    return response.data;
  },

  async getWorkspace(workspaceId: string): Promise<Workspace> {
    const response = await apiClient.get(`/${workspaceId}/details`);
    return response.data;
  },

  async getWorkspaceBranches(workspaceId: string): Promise<WorkspaceBranchesResponse> {
    const response = await apiClient.get(`/${workspaceId}/branches`);
    return response.data;
  },

  async getWorkspaceStatus(workspace_id: string, branch_name?: string): Promise<ProjectStatus> {
    const response = await apiClient.get<ProjectStatus>(
      `/${workspace_id}/status`,
      branch_name ? { params: { branch: branch_name } } : undefined
    );
    return response.data;
  },

  async switchWorkspaceBranch(
    workspaceId: string,
    branchName: string,
    baseBranch?: string
  ): Promise<WorkspaceBranch> {
    const response = await apiClient.post(`/${workspaceId}/switch-branch`, {
      branch: branchName,
      ...(baseBranch ? { base_branch: baseBranch } : {})
    });
    return response.data;
  },

  async switchWorkspaceActiveBranch(
    workspaceId: string,
    branchName: string
  ): Promise<WorkspaceBranch> {
    const response = await apiClient.post(`/${workspaceId}/switch-active-branch`, {
      branch: branchName
    });
    return response.data;
  },

  async pullChanges(
    workspaceId: string,
    branchName: string
  ): Promise<{ success: boolean; message: string }> {
    const response = await apiClient.post(`/${workspaceId}/pull-changes`, null, {
      params: { branch: branchName }
    });
    return response.data;
  },

  async continueRebase(
    workspaceId: string,
    branchName: string
  ): Promise<{ success: boolean; message: string }> {
    const response = await apiClient.post(`/${workspaceId}/continue-rebase`, null, {
      params: { branch: branchName }
    });
    return response.data;
  },

  async resolveConflictWithContent(
    workspaceId: string,
    branchName: string,
    filePath: string,
    content: string
  ): Promise<{ success: boolean; message: string }> {
    const response = await apiClient.post(
      `/${workspaceId}/resolve-conflict-with-content`,
      { content },
      { params: { branch: branchName, file: filePath } }
    );
    return response.data;
  },

  async unresolveConflictFile(
    workspaceId: string,
    branchName: string,
    filePath: string
  ): Promise<{ success: boolean; message: string }> {
    const response = await apiClient.post(`/${workspaceId}/unresolve-conflict-file`, null, {
      params: { branch: branchName, file: filePath }
    });
    return response.data;
  },

  async resolveConflictFile(
    workspaceId: string,
    branchName: string,
    filePath: string,
    side: "mine" | "theirs"
  ): Promise<{ success: boolean; message: string }> {
    const response = await apiClient.post(`/${workspaceId}/resolve-conflict-file`, null, {
      params: { branch: branchName, file: filePath, side }
    });
    return response.data;
  },

  async abortRebase(
    workspaceId: string,
    branchName: string
  ): Promise<{ success: boolean; message: string }> {
    const response = await apiClient.post(`/${workspaceId}/abort-rebase`, null, {
      params: { branch: branchName }
    });
    return response.data;
  },

  async pushChanges(
    workspaceId: string,
    branchName: string,
    commitMessage?: string
  ): Promise<{ success: boolean; message: string }> {
    const response = await apiClient.post(
      `/${workspaceId}/push-changes`,
      { commit_message: commitMessage },
      { params: { branch: branchName } }
    );
    return response.data;
  },

  async updateGitHubToken(
    token: string,
    workspaceId: string
  ): Promise<{ success: boolean; message: string }> {
    const response = await apiClient.post(`/${workspaceId}/git-token`, { token });
    return response.data;
  },

  async deleteBranch(
    workspaceId: string,
    branchName: string
  ): Promise<{ success: boolean; message: string }> {
    const response = await apiClient.delete(
      `/${workspaceId}/branches/${encodeURIComponent(branchName)}`
    );
    return response.data;
  },

  async forcePushBranch(
    workspaceId: string,
    branchName: string
  ): Promise<{ success: boolean; message: string }> {
    const response = await apiClient.post(`/${workspaceId}/force-push`, null, {
      params: { branch: branchName }
    });
    return response.data;
  },

  async getRecentCommits(workspaceId: string, branchName: string): Promise<RecentCommitsResponse> {
    const response = await apiClient.get(`/${workspaceId}/recent-commits`, {
      params: { branch: branchName }
    });
    return response.data;
  },

  async resetToCommit(
    workspaceId: string,
    branchName: string,
    commit: string
  ): Promise<{ success: boolean; message: string }> {
    const response = await apiClient.post(`/${workspaceId}/reset-to-commit`, null, {
      params: { branch: branchName, commit }
    });
    return response.data;
  },

  async createRepoFromWorkspace(
    workspaceId: string,
    gitNamespaceId: string,
    repoName: string
  ): Promise<{ success: boolean; message: string }> {
    const response = await apiClient.post(`/${workspaceId}/create-repo`, {
      git_namespace_id: gitNamespaceId,
      repo_name: repoName
    });
    return response.data;
  },

  async listAllWorkspaces(): Promise<WorkspaceSummary[]> {
    const response = await apiClient.get("/workspaces");
    return response.data;
  },

  async deleteWorkspace(workspaceId: string, deleteFiles = false): Promise<void> {
    await apiClient.delete(`/workspaces/${workspaceId}`, {
      params: { delete_files: deleteFiles }
    });
  },

  async activateWorkspace(workspaceId: string): Promise<void> {
    await apiClient.post(`/workspaces/${workspaceId}/activate`);
  },

  async renameWorkspace(workspaceId: string, name: string): Promise<void> {
    await apiClient.patch(`/workspaces/${workspaceId}/rename`, { name });
  }
};

export interface WorkspaceSummary {
  id: string;
  name: string;
  path: string | null;
  created_at: string;
  last_opened_at: string | null;
  active: boolean;
  created_by_name: string | null;
  is_cloning: boolean;
  clone_error?: string;
  agent_count: number;
  workflow_count: number;
  app_count: number;
  git_remote: string | null;
  git_commit: string | null;
  git_updated_at: string | null;
}
