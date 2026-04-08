import { useQuery } from "@tanstack/react-query";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { FileService } from "@/services/api";
import type { FileTreeResponse } from "@/types/file";
import queryKeys from "../queryKey";

export default function useFileTree(
  enabled = true,
  refetchOnWindowFocus = true,
  refetchOnMount: boolean | "always" = false
) {
  const { project, branchName } = useCurrentProjectBranch();

  return useQuery<FileTreeResponse>({
    queryKey: queryKeys.file.tree(project.id, branchName),
    queryFn: () => FileService.getFileTree(project.id, branchName),
    enabled,
    refetchOnWindowFocus: refetchOnWindowFocus,
    refetchOnMount
  });
}
