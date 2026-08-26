import { useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { type PreaggRebuildRequest, rebuildPreagg } from "@/services/api/semantic";
import queryKeys from "./queryKey";

/**
 * Trigger a pre-aggregation rebuild — one rollup, or all of them.
 *
 * The server returns as soon as the work is submitted, so success here means
 * "started", not "built". The rollups land in the manifest one at a time; the
 * caller refetches the status to watch them arrive.
 */
export default function useRebuildPreagg() {
  const { project, branchName } = useCurrentProjectBranch();
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (body: PreaggRebuildRequest) => rebuildPreagg(project.id, body, branchName),
    onSuccess: (res, body) => {
      const what = body.rollup ? `${body.view}.${body.rollup}` : `${res.rollups} rollups`;
      toast.success(`Rebuilding ${what}…`);
      queryClient.invalidateQueries({
        queryKey: queryKeys.preagg.status(project.id, branchName)
      });
    },
    onError: (e: { response?: { data?: { message?: string } } }) => {
      toast.error(e.response?.data?.message ?? "Failed to start the rebuild.");
    }
  });
}
