import { useQuery } from "@tanstack/react-query";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { CoordinatorService } from "@/services/api/coordinator";
import queryKeys from "../queryKey";

interface RunHistoryParams {
  limit: number;
  offset: number;
  status?: string;
  source_type?: string;
}

const useRunHistory = (params: RunHistoryParams) => {
  const { project } = useCurrentProjectBranch();
  const projectId = project.id;

  return useQuery({
    queryKey: queryKeys.coordinator.runHistory(projectId, params),
    queryFn: () => CoordinatorService.getRunHistory(projectId, params)
  });
};

export default useRunHistory;
