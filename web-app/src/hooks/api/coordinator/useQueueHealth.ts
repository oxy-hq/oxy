import { useQuery } from "@tanstack/react-query";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { CoordinatorService } from "@/services/api/coordinator";
import queryKeys from "../queryKey";

const useQueueHealth = () => {
  const { project } = useCurrentProjectBranch();
  const projectId = project.id;

  return useQuery({
    queryKey: queryKeys.coordinator.queue(projectId),
    queryFn: () => CoordinatorService.getQueueHealth(projectId),
    refetchInterval: 10000
  });
};

export default useQueueHealth;
