import { useQuery } from "@tanstack/react-query";

import queryKeys from "../queryKey";
import { FileService } from "@/services/api";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";

export default function useDiffSummary(enabled = true) {
  const { project, branchName } = useCurrentProjectBranch();

  return useQuery({
    queryKey: queryKeys.file.diffSummary(project.id, branchName),
    queryFn: () => FileService.getDiffSummary(project.id, branchName),
    enabled,
    refetchOnWindowFocus: false,
    refetchOnMount: true,
    staleTime: 0,
  });
}
