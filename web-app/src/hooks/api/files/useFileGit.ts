import { useQuery } from "@tanstack/react-query";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { FileService } from "@/services/api";
import queryKeys from "../queryKey";

export default function useFileGit(
  pathb64: string,
  enabled = true,
  commit = "HEAD",
  refetchOnWindowFocus = false,
  refetchOnMount: boolean | "always" = true
) {
  const { project, branchName } = useCurrentProjectBranch();

  return useQuery({
    queryKey: queryKeys.file.getGit(project.id, branchName, pathb64, commit),
    queryFn: () => FileService.getFileFromGit(project.id, pathb64, branchName, commit),
    enabled,
    refetchOnWindowFocus: refetchOnWindowFocus,
    refetchOnMount
  });
}
