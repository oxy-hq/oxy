import { useIDE } from "@/pages/ide";
import useCurrentWorkspace from "@/stores/useCurrentWorkspace";
import useIdeBranch from "@/stores/useIdeBranch";
import type { GitCapabilities } from "@/types/workspace";

const NO_GIT_CAPABILITIES: GitCapabilities = {
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

export default function useCurrentWorkspaceBranch() {
  const { workspace } = useCurrentWorkspace();
  const { insideIDE } = useIDE();

  const active_branch = workspace?.active_branch?.name ?? "";

  const { getCurrentBranch } = useIdeBranch();
  const ideBranch = (workspace ? getCurrentBranch(workspace.id) : undefined) ?? active_branch;

  const selectedBranch = insideIDE ? ideBranch : active_branch;

  const defaultBranch = workspace?.default_branch ?? "main";
  const protectedBranches = workspace?.protected_branches ?? [defaultBranch];
  const capabilities = workspace?.capabilities ?? NO_GIT_CAPABILITIES;

  // When the workspace is connected to a remote and the user is on a protected
  // branch, edits are allowed but saving auto-creates a feature branch.
  const isMainEditMode = !!(
    capabilities.auto_feature_branch_on_protected &&
    selectedBranch &&
    protectedBranches.includes(selectedBranch)
  );

  // Git UI is enabled whenever there is a local repo (commit/history/branches work).
  const gitEnabled = capabilities.can_commit;

  return {
    // Non-null assertion: this hook is only used inside authenticated/workspace-scoped routes
    // where workspace is guaranteed to be set. The null case is handled at the route level.
    workspace: workspace!,
    branchName: selectedBranch,
    capabilities,
    isMainEditMode,
    gitEnabled
  };
}
