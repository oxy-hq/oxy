import { useQuery } from "@tanstack/react-query";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { ModelingService } from "@/services/api/modeling";
import queryKeys from "../queryKey";

export default function useModelingProjects() {
  const { project, branchName } = useCurrentProjectBranch();

  return useQuery({
    queryKey: queryKeys.modeling.projects(project.id, branchName),
    queryFn: () => ModelingService.listProjects(project.id, branchName)
  });
}
