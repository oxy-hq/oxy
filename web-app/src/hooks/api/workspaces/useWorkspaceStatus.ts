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
    enabled: !!workspace.id && !!branchName
  });
};
