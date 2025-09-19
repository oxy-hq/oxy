import { RevisionInfo } from "@/types/settings";
import { useQuery } from "@tanstack/react-query";
import queryKeys from "../queryKey";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { ProjectService } from "@/services/api";

const useRevisionInfo = (
  enabled = true,
  refetchOnWindowFocus = false,
  refetchOnMount: boolean | "always" = false,
) => {
  const { project, branchName } = useCurrentProjectBranch();
  return useQuery<RevisionInfo, Error>({
    queryKey: queryKeys.projects.revisionInfo(
      project?.id || "",
      branchName || "",
    ),
    queryFn: () =>
      ProjectService.getGithubRevisionInfo(project!.id, branchName!),
    enabled,
    refetchOnWindowFocus,
    refetchOnMount,
    retry: 2,
    staleTime: 30000,
  });
};

export default useRevisionInfo;
