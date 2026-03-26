import { useQuery } from "@tanstack/react-query";
import { FileService } from "@/services/api";
import useCurrentProject from "@/stores/useCurrentProject";
import useIdeBranch from "@/stores/useIdeBranch";
import queryKeys from "../queryKey";

export default function useDiffSummary(enabled = true) {
  const { project } = useCurrentProject();
  const { getCurrentBranch } = useIdeBranch();

  const activeBranch = project?.active_branch?.name ?? "";
  const branchName = project ? (getCurrentBranch(project.id) ?? activeBranch) : "";

  return useQuery({
    queryKey: queryKeys.file.diffSummary(project?.id ?? "", branchName),
    queryFn: () => FileService.getDiffSummary(project?.id ?? "", branchName),
    enabled: enabled && !!project?.id && !!branchName,
    refetchOnWindowFocus: true,
    refetchOnMount: true,
    refetchInterval: 15_000,
    staleTime: 0
  });
}
