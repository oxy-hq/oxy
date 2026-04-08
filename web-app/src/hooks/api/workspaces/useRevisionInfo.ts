import { useQuery } from "@tanstack/react-query";
import { WorkspaceService } from "@/services/api/workspaces";
import useCurrentWorkspace from "@/stores/useCurrentWorkspace";
import useIdeBranch from "@/stores/useIdeBranch";
import type { RevisionInfo } from "@/types/settings";
import queryKeys from "../queryKey";

const useRevisionInfo = (enabled = true, refetchOnMount: boolean | "always" = true) => {
  const { workspace } = useCurrentWorkspace();
  const { getCurrentBranch } = useIdeBranch();

  const activeBranch = workspace?.active_branch?.name ?? "";
  const branchName = workspace ? (getCurrentBranch(workspace.id) ?? activeBranch) : "";

  return useQuery<RevisionInfo, Error>({
    queryKey: queryKeys.workspaces.revisionInfo(workspace?.id ?? "", branchName),
    queryFn: () => WorkspaceService.getGithubRevisionInfo(workspace?.id ?? "", branchName),
    enabled: enabled && !!workspace?.id && !!branchName,
    refetchOnWindowFocus: true,
    refetchOnMount,
    refetchInterval: enabled ? 15_000 : false,
    retry: 2,
    staleTime: 15_000
  });
};

export default useRevisionInfo;
