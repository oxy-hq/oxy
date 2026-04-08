import { useQuery } from "@tanstack/react-query";
import { WorkspaceService as ProjectService } from "@/services/api/workspaces";
import useCurrentWorkspace from "@/stores/useCurrentWorkspace";
import useIdeBranch from "@/stores/useIdeBranch";
import type { RevisionInfo } from "@/types/settings";
import queryKeys from "../queryKey";

const useRevisionInfo = (enabled = true, refetchOnMount: boolean | "always" = true) => {
  const { workspace: project } = useCurrentWorkspace();
  const { getCurrentBranch } = useIdeBranch();

  const activeBranch = project?.active_branch?.name ?? "";
  const branchName = project ? (getCurrentBranch(project.id) ?? activeBranch) : "";

  return useQuery<RevisionInfo, Error>({
    queryKey: queryKeys.workspaces.revisionInfo(project?.id ?? "", branchName),
    queryFn: () => ProjectService.getGithubRevisionInfo(project?.id ?? "", branchName),
    enabled: enabled && !!project?.id && !!branchName,
    refetchOnWindowFocus: true,
    refetchOnMount,
    refetchInterval: enabled ? 15_000 : false,
    retry: 2,
    staleTime: 15_000
  });
};

export default useRevisionInfo;
