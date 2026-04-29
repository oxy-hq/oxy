import { useQuery } from "@tanstack/react-query";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { ModelingService } from "@/services/api/modeling";
import queryKeys from "../queryKey";

export default function useModelingProject(modelingProjectName: string) {
  const { project, branchName } = useCurrentProjectBranch();

  return useQuery({
    queryKey: queryKeys.modeling.project(project.id, modelingProjectName, branchName),
    queryFn: () => ModelingService.getProjectInfo(project.id, modelingProjectName, branchName),
    enabled: !!modelingProjectName,
    retry: false
  });
}
