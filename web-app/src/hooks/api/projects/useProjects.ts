import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { ProjectService } from "@/services/api";
import queryKeys from "../queryKey";
import {
  Project,
  ProjectsResponse,
  CreateProjectRequest,
  ProjectBranchesResponse,
  CreateProjectResponse,
} from "@/types/project";

// Hook to fetch projects for an organization
export const useProjects = (organizationId: string) => {
  return useQuery<ProjectsResponse>({
    queryKey: queryKeys.projects.list(organizationId),
    queryFn: () => ProjectService.listProjects(organizationId),
    enabled: !!organizationId,
  });
};

// Hook to fetch a single project
export const useProject = (projectId: string) => {
  return useQuery<Project>({
    queryKey: queryKeys.projects.item(projectId),
    queryFn: () => ProjectService.getProject(projectId),
    enabled: !!projectId,
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

// Hook to create a project
export const useCreateProject = (organizationId: string) => {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (data: CreateProjectRequest): Promise<CreateProjectResponse> =>
      ProjectService.createProject(organizationId, data),
    onSuccess: () => {
      // Invalidate projects list to refetch
      queryClient.invalidateQueries({
        queryKey: queryKeys.projects.list(organizationId),
      });
    },
  });
};

// Hook to delete a project
export const useDeleteProject = (organizationId: string) => {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (projectId: string): Promise<void> =>
      ProjectService.deleteProject(organizationId, projectId),
    onSuccess: () => {
      // Invalidate projects list to refetch
      queryClient.invalidateQueries({
        queryKey: queryKeys.projects.list(organizationId),
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
