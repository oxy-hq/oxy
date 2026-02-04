import { useQuery } from "@tanstack/react-query";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { ProjectService } from "@/services/api";
import type { ProjectStatus } from "@/types/github";
import queryKeys from "../queryKey";

export const useProjectStatus = () => {
  const { project, branchName } = useCurrentProjectBranch();
  return useQuery<ProjectStatus>({
    queryKey: queryKeys.projects.status(project.id, branchName),
    queryFn: () => ProjectService.getProjectStatus(project.id, branchName),
    enabled: !!project.id && !!branchName
  });
};
