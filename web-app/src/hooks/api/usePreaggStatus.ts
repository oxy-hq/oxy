import { useQuery } from "@tanstack/react-query";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { getPreaggStatus } from "@/services/api/semantic";
import queryKeys from "./queryKey";

/**
 * Pre-aggregation status for the current project/branch.
 *
 * `refetchIntervalMs` is for watching a rebuild land: rollups appear in the
 * manifest one at a time, so a caller that triggered one polls until the rows
 * it is waiting on change. Leave it off the rest of the time — this reads the
 * IDE node's disk on every call.
 */
export default function usePreaggStatus({
  refetchIntervalMs
}: {
  refetchIntervalMs?: number;
} = {}) {
  const { project, branchName } = useCurrentProjectBranch();

  return useQuery({
    queryKey: queryKeys.preagg.status(project.id, branchName),
    queryFn: () => getPreaggStatus(project.id, branchName),
    staleTime: refetchIntervalMs ? 0 : 30_000,
    refetchInterval: refetchIntervalMs ?? false,
    // A 503 here is "not compiled yet" or "the IDE node is mid-deploy" — the
    // documented retryable shape, not a broken workspace. Retrying it is the
    // difference between the tab filling in a moment later and it showing
    // "Could not read the pre-aggregation cache status." until someone
    // reloads. A 4xx is a real answer and is never retried.
    retry: (failureCount, error: { response?: { status?: number } }) => {
      const status = error?.response?.status;
      if (status !== undefined && status < 500) return false;
      return failureCount < 3;
    }
  });
}
