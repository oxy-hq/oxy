import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { CompilesService, type RunCompileRequest } from "@/services/api/compiles";
import queryKeys from "./queryKey";

const DEFAULT_INTERVAL_MS = 5_000;

/**
 * Polls `GET /admin/compiles`. 5s default cadence — same as the lease
 * + internal-jobs pages — so the "started 3s ago" column on a
 * compiling row actually ticks.
 */
export const useCompiles = (
  params: { limit?: number; workspace_id?: string; status?: string } = {},
  options: { paused?: boolean; intervalMs?: number } = {}
) => {
  const { paused = false, intervalMs = DEFAULT_INTERVAL_MS } = options;
  return useQuery({
    queryKey: queryKeys.compiles.list(params),
    queryFn: () => CompilesService.list(params),
    refetchInterval: paused ? false : intervalMs
  });
};

export const useCompileDetail = (
  revisionId: string | undefined,
  options: { paused?: boolean; intervalMs?: number } = {}
) => {
  const { paused = false, intervalMs = DEFAULT_INTERVAL_MS } = options;
  return useQuery({
    queryKey: queryKeys.compiles.detail(revisionId ?? ""),
    queryFn: () => CompilesService.detail(revisionId as string),
    enabled: Boolean(revisionId),
    refetchInterval: paused ? false : intervalMs
  });
};

export const useRunCompileNow = () => {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (req: RunCompileRequest) => CompilesService.runNow(req),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: queryKeys.compiles.all });
    }
  });
};
