import { useQuery } from "@tanstack/react-query";

import queryKeys from "../queryKey";
import { FileService } from "@/services/api";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";

export default function useFileGit(
  pathb64: string,
  commit = "HEAD",
  enabled = true,
  refetchOnWindowFocus = false,
  refetchOnMount: boolean | "always" = true,
) {
  const { project, branchName } = useCurrentProjectBranch();

  return useQuery({
    queryKey: queryKeys.file.getGit(project.id, branchName, pathb64, commit),
    queryFn: () =>
      FileService.getFileFromGit(project.id, pathb64, branchName, commit),
    enabled,
    refetchOnWindowFocus: refetchOnWindowFocus,
    refetchOnMount,
  });
}
