import { useAuth } from "@/contexts/AuthContext";
import { useIDE } from "@/pages/ide";
import useCurrentWorkspace from "@/stores/useCurrentWorkspace";
import useIdeBranch from "@/stores/useIdeBranch";

export default function useCurrentWorkspaceBranch() {
  const { authConfig } = useAuth();
  const { workspace } = useCurrentWorkspace();
  const { insideIDE } = useIDE();

  const active_branch = workspace?.active_branch?.name ?? "";

  const { getCurrentBranch } = useIdeBranch();
  const ideBranch = (workspace ? getCurrentBranch(workspace.id) : undefined) ?? active_branch;

  const selectedBranch = insideIDE ? ideBranch : active_branch;

  // When local git has a remote and the user is on a protected branch, we enter
  // "force edit mode": editing is allowed freely but saving auto-creates a branch.
  // Protected branches are configured via `protected_branches` in config.yml;
  // defaults to [default_branch] (usually "main") when not set.
  // Skipped in single-workspace mode (`oxy serve --local`): a single developer
  // on their own machine expects direct writes to main, not a PR workflow.
  const protectedBranches = authConfig.protected_branches ?? [authConfig.default_branch ?? "main"];
  const isMainEditMode = !!(
    authConfig.local_git &&
    authConfig.git_remote &&
    !authConfig.single_workspace &&
    selectedBranch &&
    protectedBranches.includes(selectedBranch)
  );

  const isReadOnly = authConfig.readonly;

  // Git UI is enabled when local git is active.
  const gitEnabled = authConfig.local_git;

  return {
    // Non-null assertion: this hook is only used inside authenticated/workspace-scoped routes
    // where workspace is guaranteed to be set. The null case is handled at the route level.
    workspace: workspace!,
    branchName: selectedBranch,
    isReadOnly: isReadOnly,
    isMainEditMode,
    gitEnabled: gitEnabled
  };
}
