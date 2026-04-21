import { useQuery } from "@tanstack/react-query";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { CoordinatorService } from "@/services/api/coordinator";
import queryKeys from "../queryKey";

const useActiveRuns = () => {
  const { project } = useCurrentProjectBranch();
  const projectId = project.id;

  return useQuery({
    queryKey: queryKeys.coordinator.activeRuns(projectId),
    queryFn: () => CoordinatorService.getActiveRuns(projectId),
    refetchInterval: 5000
  });
};

export default useActiveRuns;
