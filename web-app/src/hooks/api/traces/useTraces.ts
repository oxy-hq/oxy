import { useQuery } from "@tanstack/react-query";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { type PaginatedTraceResponse, TracesService } from "@/services/api/traces";
import queryKeys from "../queryKey";

interface UseTracesOptions {
  limit?: number;
  offset?: number;
  status?: string;
  enabled?: boolean;
  duration?: string;
  /** Free-text search: trace id, span name, agent ref, or prompt (Theme 3). */
  search?: string;
  /** Absolute range (epoch seconds); with `to`, overrides `duration`. */
  from?: number;
  to?: number;
  /** Poll interval in ms for live-tail (Theme 3); `false`/omitted disables it. */
  refetchInterval?: number | false;
}

const useTraces = ({
  limit = 50,
  offset = 0,
  status = "all",
  enabled = true,
  duration,
  search,
  from,
  to,
  refetchInterval = false
}: UseTracesOptions = {}) => {
  const { project } = useCurrentProjectBranch();
  const projectId = project.id;
  return useQuery<PaginatedTraceResponse, Error>({
    queryKey: queryKeys.trace.list(projectId, limit, offset, status, duration, search, from, to),
    queryFn: () =>
      TracesService.listTraces(projectId, limit, offset, status, duration, search, from, to),
    enabled,
    refetchInterval
  });
};

export default useTraces;
