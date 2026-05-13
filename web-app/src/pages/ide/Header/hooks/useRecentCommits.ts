import { useState } from "react";
import type { CommitEntry } from "@/services/api";
import { WorkspaceService as ProjectService } from "@/services/api/workspaces";
import { useResetCommit } from "./useResetCommit";

interface Args {
  workspaceId?: string;
  branch?: string;
  onResetSuccess?: () => Promise<void> | void;
}

export function useRecentCommits({ workspaceId, branch, onResetSuccess }: Args) {
  const [open, setOpen] = useState(false);
  const [commits, setCommits] = useState<CommitEntry[]>([]);
  const [hasMore, setHasMore] = useState(false);
  const [loading, setLoading] = useState(false);

  const reset = useResetCommit({
    workspaceId,
    branch,
    onSuccess: async () => {
      setOpen(false);
      await onResetSuccess?.();
    }
  });

  const handleOpenChange = async (next: boolean) => {
    setOpen(next);
    if (!next || !workspaceId || !branch) return;
    setLoading(true);
    try {
      const result = await ProjectService.getRecentCommits(workspaceId, branch);
      setCommits(result.commits);
      setHasMore(result.has_more);
    } catch {
      setCommits([]);
      setHasMore(false);
    } finally {
      setLoading(false);
    }
  };

  return {
    open,
    onOpenChange: handleOpenChange,
    setOpen,
    commits,
    hasMore,
    loading,
    ...reset
  };
}
