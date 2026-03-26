import { useAuth } from "@/contexts/AuthContext";
import { useIDE } from "@/pages/ide";
import useCurrentProject from "@/stores/useCurrentProject";
import useIdeBranch from "@/stores/useIdeBranch";

export default function useCurrentProjectBranch() {
  const { authConfig } = useAuth();
  const { project } = useCurrentProject();
  const { insideIDE } = useIDE();

  if (!project) {
    throw new Error("Project is not selected");
  }

  const active_branch = project.active_branch?.name ?? "";

  const { getCurrentBranch } = useIdeBranch();
  const ideBranch = getCurrentBranch(project.id) ?? active_branch;

  const selectedBranch = insideIDE ? ideBranch : active_branch;

  if (!selectedBranch) {
    throw new Error("Branch is not selected");
  }

  // When local git has a remote and the user is on a protected branch, we enter
  // "force edit mode": editing is allowed freely but saving auto-creates a branch.
  // Protected branches are configured via `protected_branches` in config.yml;
  // defaults to [default_branch] (usually "main") when not set.
  const protectedBranches = authConfig.protected_branches ?? [authConfig.default_branch ?? "main"];
  const isMainEditMode = !!(
    authConfig.local_git &&
    !authConfig.cloud &&
    authConfig.git_remote &&
    protectedBranches.includes(selectedBranch)
  );

  const isReadOnly =
    authConfig.readonly ||
    (authConfig.cloud && project.project_repo_id && project.active_branch?.name === selectedBranch);

  // Git UI is enabled when either cloud GitHub integration or local git is active.
  const gitEnabled = authConfig.local_git || (!!project.project_repo_id && authConfig.cloud);

  return {
    project,
    branchName: selectedBranch,
    isReadOnly: isReadOnly,
    isMainEditMode,
    gitEnabled: gitEnabled
  };
}
