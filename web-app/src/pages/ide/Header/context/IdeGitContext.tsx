import { createContext, type ReactNode, useContext, useMemo } from "react";
import type { GitCapabilities } from "@/types/workspace";
import { type GitState, useGitState } from "../components/GitActions/useGitState";
import { useGithubUrls } from "../hooks/useGithubUrls";
import {
  type GitMutationActions,
  type GitMutationStatus,
  useGitMutations
} from "../hooks/useGitMutations";
import { useRefreshGitState } from "../hooks/useRefreshGitState";

export interface IdeGitValue {
  workspaceId: string | undefined;
  branch: string;
  defaultBranch: string;
  isOnMain: boolean;
  gitState: GitState;
  status: GitMutationStatus;
  actions: GitMutationActions;
  githubRepoUrl: string | null;
  prUrl: string | null;
  refresh: () => Promise<void>;
}

const IdeGitContext = createContext<IdeGitValue | null>(null);

interface IdeGitProviderProps {
  workspaceId: string | undefined;
  branch: string;
  defaultBranch: string;
  isOnMain: boolean;
  protectedBranches: string[];
  capabilities: GitCapabilities | undefined;
  children: ReactNode;
}

export function IdeGitProvider({
  workspaceId,
  branch,
  defaultBranch,
  isOnMain,
  protectedBranches,
  capabilities,
  children
}: IdeGitProviderProps) {
  const canDiff = !!capabilities?.can_diff;
  const { status, actions } = useGitMutations({
    workspaceId,
    branch,
    enableRevision: canDiff
  });

  const isProtected = isOnMain || protectedBranches.includes(branch);
  const gitState = useGitState({
    revisionInfo: status.revisionInfo,
    capabilities,
    isProtected
  });

  const { repoUrl: githubRepoUrl, prUrl } = useGithubUrls({
    remoteUrl: status.revisionInfo?.remote_url,
    branch,
    defaultBranch,
    isOnMain,
    gitSubfolder: status.revisionInfo?.git_subfolder
  });

  const refresh = useRefreshGitState(workspaceId, branch);

  const value = useMemo<IdeGitValue>(
    () => ({
      workspaceId,
      branch,
      defaultBranch,
      isOnMain,
      gitState,
      status,
      actions,
      githubRepoUrl,
      prUrl,
      refresh
    }),
    [
      workspaceId,
      branch,
      defaultBranch,
      isOnMain,
      gitState,
      status,
      actions,
      githubRepoUrl,
      prUrl,
      refresh
    ]
  );

  return <IdeGitContext.Provider value={value}>{children}</IdeGitContext.Provider>;
}

export function useIdeGit(): IdeGitValue {
  const value = useContext(IdeGitContext);
  if (!value) {
    throw new Error("useIdeGit must be called inside <IdeGitProvider>");
  }
  return value;
}
