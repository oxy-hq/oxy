import { useQuery } from "@tanstack/react-query";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { CoordinatorService } from "@/services/api/coordinator";
import queryKeys from "../queryKey";

interface ActiveRunsParams {
  /** Include system-managed daemon runs (preagg_cycle, etc.). Default off. */
  include_system?: boolean;
}

const useActiveRuns = (params: ActiveRunsParams = {}) => {
  const { project } = useCurrentProjectBranch();
  const projectId = project.id;
  const includeSystem = params.include_system ?? false;

  return useQuery({
    queryKey: queryKeys.coordinator.activeRuns(projectId, includeSystem),
    queryFn: () => CoordinatorService.getActiveRuns(projectId, { include_system: includeSystem }),
    refetchInterval: 5000
  });
};

export default useActiveRuns;
