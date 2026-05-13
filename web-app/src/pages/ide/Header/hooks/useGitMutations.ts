import type { QueryObserverResult } from "@tanstack/react-query";
import { useCallback, useMemo } from "react";
import { toast } from "sonner";
import useRevisionInfo from "@/hooks/api/workspaces/useRevisionInfo";
import {
  useDiscardAllChanges,
  useFetchRemote,
  useForcePush,
  usePushChanges
} from "@/hooks/api/workspaces/useWorkspaces";
import { WorkspaceService as ProjectService } from "@/services/api/workspaces";
import type { RevisionInfo } from "@/types/settings";
import { useRefreshGitState } from "./useRefreshGitState";

interface Args {
  workspaceId?: string;
  branch: string;
  enableRevision: boolean;
}

export interface GitMutationStatus {
  revisionInfo: RevisionInfo | undefined;
  isFetching: boolean;
  isPushing: boolean;
  isForcePushing: boolean;
  isDiscarding: boolean;
}

export interface GitMutationActions {
  refetchRevision: () => Promise<QueryObserverResult<RevisionInfo, Error>>;
  push: (commitMessage: string) => Promise<void>;
  forcePush: () => Promise<void>;
  abortRebase: () => Promise<void>;
  continueRebase: () => Promise<void>;
  fetchRemote: () => Promise<void>;
  discardAll: () => Promise<void>;
}

export interface UseGitMutationsResult {
  status: GitMutationStatus;
  actions: GitMutationActions;
}

interface GitActionResult {
  success: boolean;
  message?: string;
}

// Server-provided `result.message` overrides `errorMsg` on non-success;
// the catch path falls back to `errorMsg` verbatim.
async function gitActionToast(
  errorMsg: string,
  run: () => Promise<GitActionResult>,
  successMsg?: string
): Promise<GitActionResult> {
  try {
    const result = await run();
    if (result.success) {
      if (successMsg) toast.success(successMsg);
      return result;
    }
    toast.error(result.message || errorMsg);
    return result;
  } catch {
    toast.error(errorMsg);
    return { success: false };
  }
}

export function useGitMutations({
  workspaceId,
  branch,
  enableRevision
}: Args): UseGitMutationsResult {
  const {
    data: revisionInfo,
    refetch: refetchRevision,
    isFetching: isRevisionFetching
  } = useRevisionInfo(enableRevision && !!workspaceId);

  const { mutateAsync: pushMutateAsync, isPending: isPushing } = usePushChanges();
  const { mutateAsync: forcePushMutateAsync, isPending: isForcePushing } = useForcePush();
  const { mutateAsync: fetchMutateAsync, isPending: isFetchPending } = useFetchRemote();
  const { mutateAsync: discardMutateAsync, isPending: isDiscarding } = useDiscardAllChanges();
  const refreshGitState = useRefreshGitState(workspaceId, branch);

  const push = useCallback(
    async (commitMessage: string) => {
      if (!workspaceId || !branch) return;
      // Skip `successMsg` so we can toast the server-provided message below.
      const result = await gitActionToast("Push failed", () =>
        pushMutateAsync({ workspaceId, branchName: branch, commitMessage })
      );
      if (result.success) {
        toast.success(result.message || "Changes pushed");
        await refetchRevision();
      }
    },
    [workspaceId, branch, pushMutateAsync, refetchRevision]
  );

  const forcePush = useCallback(async () => {
    if (!workspaceId || !branch) return;
    const result = await gitActionToast(
      "Force push failed",
      () => forcePushMutateAsync({ workspaceId, branchName: branch }),
      "Force pushed successfully"
    );
    if (result.success) void refetchRevision();
  }, [workspaceId, branch, forcePushMutateAsync, refetchRevision]);

  const abortRebase = useCallback(async () => {
    if (!workspaceId || !branch) return;
    const result = await gitActionToast(
      "Failed to abort",
      () => ProjectService.abortRebase(workspaceId, branch),
      "Rebase aborted — branch restored to previous state"
    );
    if (result.success) await refreshGitState();
  }, [workspaceId, branch, refreshGitState]);

  const continueRebase = useCallback(async () => {
    if (!workspaceId || !branch) return;
    const result = await gitActionToast(
      "Failed to continue rebase",
      () => ProjectService.continueRebase(workspaceId, branch),
      "Conflicts resolved — rebase complete"
    );
    if (result.success) await refreshGitState();
  }, [workspaceId, branch, refreshGitState]);

  const fetchRemote = useCallback(async () => {
    if (!workspaceId || !branch) return;
    const result = await gitActionToast("Fetch failed", () =>
      fetchMutateAsync({ workspaceId, branchName: branch })
    );
    if (result.success) await refetchRevision();
  }, [workspaceId, branch, fetchMutateAsync, refetchRevision]);

  const discardAll = useCallback(async () => {
    if (!workspaceId || !branch) return;
    // Skip `successMsg` so we can toast the server-provided message below.
    const result = await gitActionToast("Failed to discard changes", () =>
      discardMutateAsync({ workspaceId, branchName: branch })
    );
    if (result.success) {
      toast.success(result.message || "Discarded all local changes");
      await refreshGitState();
    }
  }, [workspaceId, branch, discardMutateAsync, refreshGitState]);

  const status = useMemo<GitMutationStatus>(
    () => ({
      revisionInfo,
      isFetching: isRevisionFetching || isFetchPending,
      isPushing,
      isForcePushing,
      isDiscarding
    }),
    [revisionInfo, isRevisionFetching, isFetchPending, isPushing, isForcePushing, isDiscarding]
  );

  const actions = useMemo<GitMutationActions>(
    () => ({
      refetchRevision,
      push,
      forcePush,
      abortRebase,
      continueRebase,
      fetchRemote,
      discardAll
    }),
    [refetchRevision, push, forcePush, abortRebase, continueRebase, fetchRemote, discardAll]
  );

  return { status, actions };
}
