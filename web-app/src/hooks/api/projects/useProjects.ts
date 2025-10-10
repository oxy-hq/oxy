import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { ProjectService } from "@/services/api";
import queryKeys from "../queryKey";
import { Project, ProjectBranchesResponse } from "@/types/project";

const getLocalProject = (): Project => ({
  id: "00000000-0000-0000-0000-000000000000",
  name: "Local Development Project",
  workspace_id: "00000000-0000-0000-0000-000000000000",
  project_repo_id: "00000000-0000-0000-0000-000000000000",
  active_branch: {
    name: "main",
    sync_status: "synced",
    revision: "00000000-0000-0000-0000-000000000000",
    id: "00000000-0000-0000-0000-000000000000",
    created_at: new Date().toISOString(),
    updated_at: new Date().toISOString(),
    branch_type: "local",
  },
  created_at: new Date().toISOString(),
  updated_at: new Date().toISOString(),
});

// Hook to fetch a single project
export const useProject = (projectId: string, cloud: boolean) => {
  return useQuery<Project>({
    queryKey: queryKeys.projects.item(projectId),
    queryFn: () =>
      cloud ? ProjectService.getProject(projectId) : getLocalProject(),
    enabled: !cloud || !!projectId,
  });
};

// Hook to fetch project branches
export const useProjectBranches = (projectId: string) => {
  return useQuery<ProjectBranchesResponse>({
    queryKey: queryKeys.projects.branches(projectId),
    queryFn: () => ProjectService.getProjectBranches(projectId),
    enabled: !!projectId,
  });
};

// Hook to delete a project
export const useDeleteProject = (workspaceId: string) => {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (projectId: string): Promise<void> =>
      ProjectService.deleteProject(workspaceId, projectId),
    onSuccess: () => {
      // Invalidate projects list to refetch
      queryClient.invalidateQueries({
        queryKey: queryKeys.projects.list(workspaceId),
      });
    },
  });
};

// Hook to switch project branch
export const useSwitchProjectBranch = () => {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({
      projectId,
      branchName,
    }: {
      projectId: string;
      branchName: string;
    }) => ProjectService.switchProjectBranch(projectId, branchName),
    onSuccess: (_, variables) => {
      // Invalidate project details and branches to refetch
      queryClient.invalidateQueries({
        queryKey: queryKeys.projects.item(variables.projectId),
      });
      queryClient.invalidateQueries({
        queryKey: queryKeys.projects.branches(variables.projectId),
      });
    },
  });
};

// Hook to pull changes
export const usePullChanges = () => {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({
      projectId,
      branchName,
    }: {
      projectId: string;
      branchName: string;
    }) => ProjectService.pullChanges(projectId, branchName),
    onSuccess: (_, variables) => {
      // Invalidate revision info to refetch after pull
      queryClient.invalidateQueries({
        queryKey: queryKeys.projects.revisionInfo(
          variables.projectId,
          variables.branchName,
        ),
      });
      queryClient.invalidateQueries({
        queryKey: queryKeys.file.all(variables.projectId, variables.branchName),
      });
    },
  });
};

// Hook to push changes
export const usePushChanges = () => {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({
      projectId,
      branchName,
      commitMessage,
    }: {
      projectId: string;
      branchName: string;
      commitMessage?: string;
    }) => ProjectService.pushChanges(projectId, branchName, commitMessage),
    onSuccess: (_, variables) => {
      // Invalidate revision info to refetch after push
      queryClient.invalidateQueries({
        queryKey: queryKeys.projects.revisionInfo(
          variables.projectId,
          variables.branchName,
        ),
      });
      queryClient.invalidateQueries({
        queryKey: queryKeys.file.all(variables.projectId, variables.branchName),
      });
    },
  });
};
