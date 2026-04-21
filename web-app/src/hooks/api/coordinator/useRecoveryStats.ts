import { useQuery } from "@tanstack/react-query";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { CoordinatorService } from "@/services/api/coordinator";
import queryKeys from "../queryKey";

const useRecoveryStats = () => {
  const { project } = useCurrentProjectBranch();
  const projectId = project.id;

  return useQuery({
    queryKey: queryKeys.coordinator.recovery(projectId),
    queryFn: () => CoordinatorService.getRecoveryStats(projectId)
  });
};

export default useRecoveryStats;
