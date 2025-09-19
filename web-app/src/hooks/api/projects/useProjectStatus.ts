import { ProjectService } from "@/services/api";
import queryKeys from "../queryKey";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { useQuery } from "@tanstack/react-query";
import { ProjectStatus } from "@/types/github";

export const useProjectStatus = () => {
  const { project, branchName } = useCurrentProjectBranch();
  return useQuery<ProjectStatus>({
    queryKey: queryKeys.projects.status(project.id, branchName),
    queryFn: () => ProjectService.getProjectStatus(project.id, branchName),
    enabled: !!project.id && !!branchName,
  });
};
