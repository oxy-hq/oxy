import { useState } from "react";
import { toast } from "sonner";
import type { CommitEntry, DirtyEntry } from "@/services/api";
import { WorkspaceService as ProjectService } from "@/services/api/workspaces";
import { useRefreshGitState } from "./useRefreshGitState";

interface Args {
  workspaceId?: string;
  branch?: string;
  onSuccess?: () => Promise<void> | void;
}

export interface PendingReset {
  hash: string;
  shortHash: string;
  /** Uncommitted files that would be discarded. Empty for a commit-loss refusal. */
  dirty: DirtyEntry[];
  /** Commits that would be dropped. Empty for a dirty-tree refusal. */
  discardedCommits: CommitEntry[];
}

/**
 * First call probes the guards and surfaces `pendingReset` so the caller can
 * confirm; `confirmReset` re-issues with `force=true`.
 *
 * Both server-side guards are recoverable, so both must produce a confirmable
 * `pendingReset`. Previously only the dirty-tree refusal did, and the
 * commit-loss refusal — the more consequential of the two — degraded to a
 * transient toast telling the user to "re-run with force", an action the UI
 * offered no way to perform. See oxygen-workspace-sync-bugs.md bug 2.
 */
export function useResetCommit({ workspaceId, branch, onSuccess }: Args) {
  const [resettingHash, setResettingHash] = useState<string | null>(null);
  const [pendingReset, setPendingReset] = useState<PendingReset | null>(null);
  const refreshGitState = useRefreshGitState(workspaceId, branch);

  const performReset = async (hash: string, force: boolean) => {
    if (!workspaceId || !branch) return;
    setResettingHash(hash);
    try {
      const result = await ProjectService.resetToCommit(workspaceId, branch, hash, force);
      if (result.success) {
        toast.success(`Restored to ${hash.substring(0, 7)}`);
        setPendingReset(null);
        await refreshGitState();
        await onSuccess?.();
        return true;
      }
      const dirty = result.dirty ?? [];
      const discardedCommits = result.discarded_commits ?? [];
      if (dirty.length > 0 || discardedCommits.length > 0) {
        setPendingReset({
          hash,
          shortHash: hash.substring(0, 7),
          dirty,
          discardedCommits
        });
        return false;
      }
      toast.error(result.message || "Restore failed");
      return false;
    } catch {
      toast.error("Restore failed");
      return false;
    } finally {
      setResettingHash(null);
    }
  };

  const resetToCommit = (hash: string) => performReset(hash, false);
  const confirmReset = () => {
    if (pendingReset) void performReset(pendingReset.hash, true);
  };
  const cancelReset = () => setPendingReset(null);

  return {
    resettingHash,
    pendingReset,
    resetToCommit,
    confirmReset,
    cancelReset
  };
}
