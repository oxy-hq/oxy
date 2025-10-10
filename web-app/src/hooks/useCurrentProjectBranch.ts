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

  const isReadOnly =
    authConfig.cloud &&
    project.project_repo_id &&
    project.active_branch?.name === selectedBranch;

  const gitEnabled = !!project.project_repo_id && authConfig.cloud;

  return {
    project,
    branchName: selectedBranch,
    isReadOnly: isReadOnly,
    gitEnabled: gitEnabled,
  };
}
