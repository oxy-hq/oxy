import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";

import { CompileService } from "@/services/api/compile";
import queryKeys from "./queryKey";

const POLL_WHILE_INFLIGHT_MS = 5_000;

/**
 * Polls the workspace's compile status. The query refetches every
 * `POLL_WHILE_INFLIGHT_MS` while a compile is in flight (latest.status
 * is `compiling`) and stops once the latest revision has settled.
 */
export const useCompileStatus = (workspaceId: string | undefined, branch: string) => {
  return useQuery({
    queryKey: queryKeys.compile.status(workspaceId ?? "", branch),
    queryFn: () => CompileService.status(workspaceId as string, branch),
    enabled: !!workspaceId && branch.length > 0,
    refetchInterval: (q) => {
      const data = q.state.data;
      return data?.latest?.status === "compiling" ? POLL_WHILE_INFLIGHT_MS : false;
    }
  });
};

/**
 * Fires a main-branch Compile. Optimistic: invalidates the status
 * query on success so the polling kicks back in immediately.
 */
export const useEnqueueCompile = (workspaceId: string | undefined, branch: string) => {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => {
      if (!workspaceId) throw new Error("No active workspace");
      return CompileService.enqueue(workspaceId, branch);
    },
    onSuccess: () => {
      toast.success("Compile started");
      if (workspaceId) {
        qc.invalidateQueries({
          queryKey: queryKeys.compile.status(workspaceId, branch)
        });
      }
    },
    onError: (err: unknown) => {
      const message = err instanceof Error ? err.message : "Failed to start compile";
      toast.error(message);
    }
  });
};
