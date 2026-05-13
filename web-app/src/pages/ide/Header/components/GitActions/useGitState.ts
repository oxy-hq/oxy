import { useMemo } from "react";
import type { RevisionInfo } from "@/types/settings";
import type { GitCapabilities } from "@/types/workspace";

export interface GitState {
  hasRemote: boolean;
  isProtected: boolean;
  isInConflict: boolean;
  uncommittedCount: number;
  aheadCount: number;
  behindCount: number;
  remoteUrl: string | undefined;
  caps: GitCapabilities;
}

interface Args {
  revisionInfo: RevisionInfo | undefined;
  capabilities: GitCapabilities | undefined;
  isProtected: boolean;
}

const EMPTY_CAPS: GitCapabilities = {
  can_commit: false,
  can_browse_history: false,
  can_reset_to_commit: false,
  can_switch_branch: false,
  can_diff: false,
  can_push: false,
  can_pull: false,
  can_fetch: false,
  can_force_push: false,
  can_rebase: false,
  can_open_pr: false,
  auto_feature_branch_on_protected: false
};

export function useGitState({ revisionInfo, capabilities, isProtected }: Args): GitState {
  return useMemo<GitState>(
    () => ({
      hasRemote: !!revisionInfo?.remote_url,
      isProtected,
      isInConflict: !!revisionInfo?.is_in_conflict,
      uncommittedCount: revisionInfo?.uncommitted_count ?? 0,
      aheadCount: revisionInfo?.ahead_count ?? 0,
      behindCount: revisionInfo?.behind_count ?? 0,
      remoteUrl: revisionInfo?.remote_url,
      caps: capabilities ?? EMPTY_CAPS
    }),
    [revisionInfo, capabilities, isProtected]
  );
}
