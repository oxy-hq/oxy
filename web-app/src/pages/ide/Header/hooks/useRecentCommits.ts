import { useState } from "react";
import { toast } from "sonner";
import type { CommitEntry } from "@/services/api";
import { WorkspaceService as ProjectService } from "@/services/api/workspaces";

interface Args {
  workspaceId?: string;
  branch?: string;
  onResetSuccess?: () => Promise<void> | void;
}

/**
 * Lazy-loaded recent-commits state for the History popover.
 * Loads commits when the popover opens; resets state to a chosen hash via
 * `resetToCommit`. Toasts on success/failure.
 */
export function useRecentCommits({ workspaceId, branch, onResetSuccess }: Args) {
  const [open, setOpen] = useState(false);
  const [commits, setCommits] = useState<CommitEntry[]>([]);
  const [loading, setLoading] = useState(false);
  const [resettingHash, setResettingHash] = useState<string | null>(null);

  const handleOpenChange = async (next: boolean) => {
    setOpen(next);
    if (!next || !workspaceId || !branch) return;
    setLoading(true);
    try {
      const result = await ProjectService.getRecentCommits(workspaceId, branch);
      setCommits(result.commits);
    } catch {
      setCommits([]);
    } finally {
      setLoading(false);
    }
  };

  const resetToCommit = async (hash: string) => {
    if (!workspaceId || !branch) return;
    setResettingHash(hash);
    try {
      const result = await ProjectService.resetToCommit(workspaceId, branch, hash);
      if (result.success) {
        toast.success(`Restored to ${hash.substring(0, 7)}`);
        setOpen(false);
        await onResetSuccess?.();
      } else {
        toast.error(result.message || "Restore failed");
      }
    } catch {
      toast.error("Restore failed");
    } finally {
      setResettingHash(null);
    }
  };

  return {
    open,
    onOpenChange: handleOpenChange,
    commits,
    loading,
    resettingHash,
    resetToCommit
  };
}
