import { useQuery } from "@tanstack/react-query";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { type PaginatedTraceResponse, TracesService } from "@/services/api/traces";
import queryKeys from "../queryKey";

const useTraces = (
  limit: number = 50,
  offset: number = 0,
  status: string = "all",
  enabled = true,
  duration?: string
) => {
  const { project } = useCurrentProjectBranch();
  const projectId = project.id;
  return useQuery<PaginatedTraceResponse, Error>({
    queryKey: queryKeys.trace.list(projectId, limit, offset, status, duration),
    queryFn: () => TracesService.listTraces(projectId, limit, offset, status, duration),
    enabled
  });
};

export default useTraces;
