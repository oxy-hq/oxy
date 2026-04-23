import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { WorkspaceService, type WorkspaceSummary } from "@/services/api/workspaces";
import useCurrentOrg from "@/stores/useCurrentOrg";
import type { Workspace, WorkspaceBranchesResponse } from "@/types/workspace";
import queryKeys from "../queryKey";

// Hook to fetch a single workspace
export const useWorkspace = (workspaceId: string) => {
  return useQuery<Workspace>({
    queryKey: queryKeys.workspaces.item(workspaceId),
    queryFn: () => WorkspaceService.getWorkspace(workspaceId)
  });
};

// Hook to fetch workspace branches
export const useWorkspaceBranches = (workspaceId: string) => {
  return useQuery<WorkspaceBranchesResponse>({
    queryKey: queryKeys.workspaces.branches(workspaceId),
    queryFn: () => WorkspaceService.getWorkspaceBranches(workspaceId),
    enabled: !!workspaceId
  });
};

// Hook to switch workspace branch
export const useSwitchWorkspaceBranch = () => {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({
      workspaceId,
      branchName,
      baseBranch
    }: {
      workspaceId: string;
      branchName: string;
      baseBranch?: string;
    }) => WorkspaceService.switchWorkspaceBranch(workspaceId, branchName, baseBranch),
    onSuccess: (_, variables) => {
      // Invalidate workspace details and branches to refetch
      queryClient.invalidateQueries({
        queryKey: queryKeys.workspaces.item(variables.workspaceId)
      });
      queryClient.invalidateQueries({
        queryKey: queryKeys.workspaces.branches(variables.workspaceId)
      });
    }
  });
};

// Hook to pull changes
export const usePullChanges = () => {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({ workspaceId, branchName }: { workspaceId: string; branchName: string }) =>
      WorkspaceService.pullChanges(workspaceId, branchName),
    onSuccess: (_, variables) => {
      // Refetch revision info immediately after pull, including inactive observers
      // so the status updates even if BranchInfo unmounts during navigation.
      queryClient.invalidateQueries({
        queryKey: queryKeys.workspaces.revisionInfo(variables.workspaceId, variables.branchName),
        refetchType: "all"
      });
      queryClient.invalidateQueries({
        queryKey: queryKeys.file.all(variables.workspaceId, variables.branchName),
        refetchType: "all"
      });
    }
  });
};

// Hook to delete a branch
export const useDeleteBranch = (workspaceId: string) => {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (branchName: string) => WorkspaceService.deleteBranch(workspaceId, branchName),
    onSuccess: () => {
      queryClient.invalidateQueries({
        queryKey: queryKeys.workspaces.branches(workspaceId)
      });
    }
  });
};

// Hook to force-push the current branch to remote
export const useForcePush = () => {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({ workspaceId, branchName }: { workspaceId: string; branchName: string }) =>
      WorkspaceService.forcePushBranch(workspaceId, branchName),
    onSuccess: (_, variables) => {
      queryClient.invalidateQueries({
        queryKey: queryKeys.workspaces.revisionInfo(variables.workspaceId, variables.branchName)
      });
    }
  });
};

// Hook to push changes
export const usePushChanges = () => {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({
      workspaceId,
      branchName,
      commitMessage
    }: {
      workspaceId: string;
      branchName: string;
      commitMessage?: string;
    }) => WorkspaceService.pushChanges(workspaceId, branchName, commitMessage),
    onSuccess: (_, variables) => {
      // Invalidate revision info to refetch after push
      queryClient.invalidateQueries({
        queryKey: queryKeys.workspaces.revisionInfo(variables.workspaceId, variables.branchName)
      });
      queryClient.invalidateQueries({
        queryKey: queryKeys.file.all(variables.workspaceId, variables.branchName)
      });
    }
  });
};

/**
 * Fetches the workspace list for an org.
 *
 * Default reads the org id from the currentOrg Zustand store. Callers that
 * already know which org they're asking about (e.g. the dispatcher which
 * computes chosenOrg before the store is primed) should pass `orgIdOverride`
 * so the query key and fetched data can never drift out of sync with the
 * store while it catches up.
 */
export const useAllWorkspaces = (orgIdOverride?: string) => {
  const storeOrgId = useCurrentOrg((s) => s.org?.id);
  const orgId = orgIdOverride ?? storeOrgId;

  return useQuery<WorkspaceSummary[]>({
    queryKey: [...queryKeys.workspaces.list(), orgId],
    queryFn: () => {
      if (!orgId) return Promise.resolve([]);
      return WorkspaceService.listAllWorkspaces(orgId);
    },
    enabled: !!orgId,
    // Poll every 3 s while any workspace is still cloning so the UI updates
    // automatically once the background git clone finishes.
    refetchInterval: (query) => {
      const data = query.state.data;
      return data?.some((p) => p.status === "cloning") ? 3000 : false;
    }
  });
};

export const useDeleteWorkspace = () => {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({
      orgId,
      id,
      deleteFiles
    }: {
      orgId: string;
      id: string;
      deleteFiles?: boolean;
    }) => WorkspaceService.deleteWorkspace(orgId, id, deleteFiles),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: queryKeys.workspaces.list() });
    }
  });
};

export const useRenameWorkspace = () => {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ orgId, id, name }: { orgId: string; id: string; name: string }) =>
      WorkspaceService.renameWorkspace(orgId, id, name),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: queryKeys.workspaces.list() });
    }
  });
};
