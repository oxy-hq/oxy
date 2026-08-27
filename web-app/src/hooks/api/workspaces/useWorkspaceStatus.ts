import { useQuery } from "@tanstack/react-query";
import useCurrentWorkspaceBranch from "@/hooks/useCurrentWorkspaceBranch";
import { WorkspaceService } from "@/services/api/workspaces";
import type { ProjectStatus } from "@/types/github";
import queryKeys from "../queryKey";

export const useWorkspaceStatus = () => {
  const { workspace, branchName } = useCurrentWorkspaceBranch();
  return useQuery<ProjectStatus>({
    queryKey: queryKeys.workspaces.status(workspace.id, branchName),
    queryFn: () => WorkspaceService.getWorkspaceStatus(workspace.id, branchName),
    // NOT gated on `branchName`. It is "" outside the IDE by design, and this
    // component renders on every non-IDE route (`WorkspaceShell` hides it only
    // inside the IDE) — so gating on it left the query permanently disabled,
    // stuck `isPending`, and `WorkspaceConfigStatus` returns null while pending.
    // The one surface that tells a user their config.yml is broken then never
    // appeared anywhere but the IDE.
    //
    // "" is a valid question, not a missing one: `getWorkspaceStatus` omits the
    // param when falsy and the server reads that as the default branch.
    enabled: !!workspace.id
  });
};
