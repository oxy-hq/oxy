import { useQuery } from "@tanstack/react-query";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { CoordinatorService } from "@/services/api/coordinator";
import queryKeys from "../queryKey";

const useRunTree = (runId: string | undefined) => {
  const { project } = useCurrentProjectBranch();
  const projectId = project.id;

  return useQuery({
    queryKey: queryKeys.coordinator.runTree(projectId, runId!),
    queryFn: () => CoordinatorService.getRunTree(projectId, runId!),
    enabled: !!runId
  });
};

export default useRunTree;
