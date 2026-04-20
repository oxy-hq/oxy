import { toast } from "sonner";
import useDiffSummary from "@/hooks/api/files/useDiffSummary";
import useRevisionInfo from "@/hooks/api/workspaces/useRevisionInfo";
import { useForcePush, usePushChanges } from "@/hooks/api/workspaces/useWorkspaces";
import { WorkspaceService as ProjectService } from "@/services/api/workspaces";

interface Args {
  workspaceId?: string;
  branch: string;
  /** Whether to fetch diff/revision info (capability-gated by the caller). */
  enableDiff: boolean;
  enableRevision: boolean;
}

/**
 * Centralised git mutation handlers + the supporting queries.
 *
 * Returns the diff/revision data so the caller doesn't need to call them
 * separately, and exposes refetch helpers used by every mutation success path.
 * Each handler toasts on success/failure.
 */
export function useGitMutations({ workspaceId, branch, enableDiff, enableRevision }: Args) {
  const {
    data: diffSummary,
    refetch: refetchDiff,
    isFetching: isDiffFetching
  } = useDiffSummary(enableDiff && !!workspaceId);
  const {
    data: revisionInfo,
    refetch: refetchRevision,
    isFetching: isRevisionFetching
  } = useRevisionInfo(enableRevision && !!workspaceId);

  const pushMutation = usePushChanges();
  const forcePushMutation = useForcePush();

  const fetchAll = async () => {
    await Promise.all([refetchDiff(), refetchRevision()]);
  };

  const push = async (commitMessage: string) => {
    if (!workspaceId || !branch) return;
    try {
      const result = await pushMutation.mutateAsync({
        workspaceId,
        branchName: branch,
        commitMessage
      });
      if (result.success) {
        toast.success(result.message || "Changes pushed");
        await fetchAll();
      } else {
        toast.error(result.message || "Push failed");
      }
    } catch {
      toast.error("Push failed");
    }
  };

  const forcePush = async () => {
    if (!workspaceId || !branch) return;
    try {
      const result = await forcePushMutation.mutateAsync({
        workspaceId,
        branchName: branch
      });
      if (result.success) {
        toast.success("Force pushed successfully");
        refetchRevision();
      } else {
        toast.error(result.message || "Force push failed");
      }
    } catch {
      toast.error("Force push failed");
    }
  };

  const abortRebase = async () => {
    if (!workspaceId || !branch) return;
    try {
      const result = await ProjectService.abortRebase(workspaceId, branch);
      if (result.success) {
        toast.success("Rebase aborted — branch restored to previous state");
        await fetchAll();
      } else {
        toast.error(result.message || "Failed to abort");
      }
    } catch {
      toast.error("Failed to abort");
    }
  };

  const continueRebase = async () => {
    if (!workspaceId || !branch) return;
    try {
      const result = await ProjectService.continueRebase(workspaceId, branch);
      if (result.success) {
        toast.success("Conflicts resolved — rebase complete");
        await fetchAll();
      } else {
        toast.error(result.message || "Failed to continue rebase");
      }
    } catch {
      toast.error("Failed to continue rebase");
    }
  };

  return {
    diffSummary,
    revisionInfo,
    isFetching: isDiffFetching || isRevisionFetching,
    isPushing: pushMutation.isPending,
    isForcePushing: forcePushMutation.isPending,
    fetchAll,
    push,
    forcePush,
    abortRebase,
    continueRebase
  };
}
