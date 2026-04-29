import { useQuery } from "@tanstack/react-query";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { ModelingService } from "@/services/api/modeling";
import queryKeys from "../queryKey";

export default function useModelingLineage(modelingProjectName: string, enabled = true) {
  const { project, branchName } = useCurrentProjectBranch();

  return useQuery({
    queryKey: queryKeys.modeling.lineage(project.id, modelingProjectName, branchName),
    queryFn: () => ModelingService.getLineage(project.id, modelingProjectName, branchName),
    enabled: enabled && !!modelingProjectName,
    retry: false
  });
}
