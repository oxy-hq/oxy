import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { WorkspaceService, type WorkspaceSummary } from "@/services/api/workspaces";
import type { Project, ProjectBranchesResponse } from "@/types/project";
import queryKeys from "../queryKey";

// Hook to fetch a single project
export const useProject = (projectId: string) => {
  return useQuery<Project>({
    queryKey: queryKeys.workspaces.item(projectId),
    queryFn: () => WorkspaceService.getWorkspace(projectId) as unknown as Promise<Project>
  });
};

// Hook to fetch project branches
export const useProjectBranches = (projectId: string) => {
  return useQuery<ProjectBranchesResponse>({
    queryKey: queryKeys.workspaces.branches(projectId),
    queryFn: () =>
      WorkspaceService.getWorkspaceBranches(
        projectId
      ) as unknown as Promise<ProjectBranchesResponse>,
    enabled: !!projectId
  });
};

// Hook to switch project branch
export const useSwitchProjectBranch = () => {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({ projectId, branchName }: { projectId: string; branchName: string }) =>
      WorkspaceService.switchWorkspaceBranch(projectId, branchName),
    onSuccess: (_, variables) => {
      // Invalidate workspace details and branches to refetch
      queryClient.invalidateQueries({
        queryKey: queryKeys.workspaces.item(variables.projectId)
      });
      queryClient.invalidateQueries({
        queryKey: queryKeys.workspaces.branches(variables.projectId)
      });
    }
  });
};

// Hook to pull changes
export const usePullChanges = () => {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({ projectId, branchName }: { projectId: string; branchName: string }) =>
      WorkspaceService.pullChanges(projectId, branchName),
    onSuccess: (_, variables) => {
      // Refetch revision info immediately after pull, including inactive observers
      // so the status updates even if BranchInfo unmounts during navigation.
      queryClient.invalidateQueries({
        queryKey: queryKeys.workspaces.revisionInfo(variables.projectId, variables.branchName),
        refetchType: "all"
      });
      queryClient.invalidateQueries({
        queryKey: queryKeys.file.all(variables.projectId, variables.branchName),
        refetchType: "all"
      });
    }
  });
};

// Hook to delete a branch
export const useDeleteBranch = (projectId: string) => {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (branchName: string) => WorkspaceService.deleteBranch(projectId, branchName),
    onSuccess: () => {
      queryClient.invalidateQueries({
        queryKey: queryKeys.workspaces.branches(projectId)
      });
    }
  });
};

// Hook to force-push the current branch to remote
export const useForcePush = () => {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({ projectId, branchName }: { projectId: string; branchName: string }) =>
      WorkspaceService.forcePushBranch(projectId, branchName),
    onSuccess: (_, variables) => {
      queryClient.invalidateQueries({
        queryKey: queryKeys.workspaces.revisionInfo(variables.projectId, variables.branchName)
      });
    }
  });
};

// Hook to push changes
export const usePushChanges = () => {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({
      projectId,
      branchName,
      commitMessage
    }: {
      projectId: string;
      branchName: string;
      commitMessage?: string;
    }) => WorkspaceService.pushChanges(projectId, branchName, commitMessage),
    onSuccess: (_, variables) => {
      // Invalidate revision info to refetch after push
      queryClient.invalidateQueries({
        queryKey: queryKeys.workspaces.revisionInfo(variables.projectId, variables.branchName)
      });
      queryClient.invalidateQueries({
        queryKey: queryKeys.file.all(variables.projectId, variables.branchName)
      });
    }
  });
};

export const useAllProjects = () => {
  return useQuery<WorkspaceSummary[]>({
    queryKey: queryKeys.workspaces.list(),
    queryFn: () => WorkspaceService.listAllWorkspaces(),
    // Poll every 3 s while any workspace is still cloning so the UI updates
    // automatically once the background git clone finishes.
    refetchInterval: (query) => {
      const data = query.state.data;
      return data?.some((p) => p.is_cloning) ? 3000 : false;
    }
  });
};

export const useDeleteProject = () => {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ id, deleteFiles }: { id: string; deleteFiles?: boolean }) =>
      WorkspaceService.deleteWorkspace(id, deleteFiles),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: queryKeys.workspaces.list() });
    }
  });
};

export const useActivateProject = () => {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (projectId: string) => WorkspaceService.activateWorkspace(projectId),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: queryKeys.workspaces.list() });
      queryClient.invalidateQueries({ queryKey: ["authConfig"] });
      // Re-bootstrap the active workspace so MainLayout loads the new workspace's details.
      queryClient.invalidateQueries({
        queryKey: queryKeys.workspaces.item("00000000-0000-0000-0000-000000000000")
      });
      // Flush all workspace-scoped data so the new workspace's content loads fresh.
      queryClient.invalidateQueries({ queryKey: queryKeys.thread.all });
      queryClient.invalidateQueries({ queryKey: queryKeys.agent.all });
      queryClient.invalidateQueries({ queryKey: queryKeys.workflow.all });
      queryClient.invalidateQueries({ queryKey: queryKeys.app.all });
      queryClient.invalidateQueries({ queryKey: queryKeys.database.all });
    }
  });
};
