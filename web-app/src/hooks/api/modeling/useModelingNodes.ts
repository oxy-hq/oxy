import { useQuery } from "@tanstack/react-query";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { ModelingService } from "@/services/api/modeling";
import queryKeys from "../queryKey";

export default function useModelingNodes(modelingProjectName: string) {
  const { project, branchName } = useCurrentProjectBranch();

  return useQuery({
    queryKey: queryKeys.modeling.nodes(project.id, modelingProjectName, branchName),
    queryFn: () => ModelingService.listNodes(project.id, modelingProjectName, branchName),
    enabled: !!modelingProjectName,
    retry: false
  });
}
