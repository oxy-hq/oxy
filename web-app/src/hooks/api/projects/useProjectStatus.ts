import { useQuery } from "@tanstack/react-query";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { WorkspaceService as ProjectService } from "@/services/api/workspaces";
import type { ProjectStatus } from "@/types/github";
import queryKeys from "../queryKey";

export const useProjectStatus = () => {
  const { project, branchName } = useCurrentProjectBranch();
  return useQuery<ProjectStatus>({
    queryKey: queryKeys.workspaces.status(project.id, branchName),
    queryFn: () => ProjectService.getWorkspaceStatus(project.id, branchName),
    enabled: !!project.id && !!branchName
  });
};
