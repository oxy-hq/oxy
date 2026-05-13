import { useState } from "react";
import { toast } from "sonner";
import type { DirtyEntry } from "@/services/api";
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
  dirty: DirtyEntry[];
}

/**
 * First call probes for uncommitted changes and surfaces `pendingReset`
 * so the caller can confirm; `confirmReset` re-issues with `force=true`.
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
      if (result.dirty && result.dirty.length > 0) {
        setPendingReset({ hash, shortHash: hash.substring(0, 7), dirty: result.dirty });
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
