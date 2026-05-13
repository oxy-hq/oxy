import { useQueryClient } from "@tanstack/react-query";
import { useCallback } from "react";
import queryKeys from "@/hooks/api/queryKey";

/**
 * Invalidate every query that depends on the working-tree state of the
 * given branch — file tree, file contents, diff summary, revision info.
 */
export function useRefreshGitState(workspaceId?: string, branchName?: string) {
  const queryClient = useQueryClient();

  return useCallback(async () => {
    if (!workspaceId || !branchName) return;
    await Promise.all([
      queryClient.invalidateQueries({
        queryKey: queryKeys.file.all(workspaceId, branchName),
        refetchType: "all"
      }),
      queryClient.invalidateQueries({
        queryKey: queryKeys.workspaces.revisionInfo(workspaceId, branchName),
        refetchType: "all"
      })
    ]);
  }, [queryClient, workspaceId, branchName]);
}
