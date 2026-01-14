import { TracesService, PaginatedTraceResponse } from "@/services/api/traces";
import { useQuery } from "@tanstack/react-query";
import queryKeys from "../queryKey";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";

const useTraces = (
  limit: number = 50,
  offset: number = 0,
  status: string = "all",
  enabled = true,
  duration?: string,
) => {
  const { project } = useCurrentProjectBranch();
  const projectId = project.id;
  return useQuery<PaginatedTraceResponse, Error>({
    queryKey: queryKeys.trace.list(projectId, limit, offset, status, duration),
    queryFn: () =>
      TracesService.listTraces(projectId, limit, offset, status, duration),
    enabled,
  });
};

export default useTraces;
