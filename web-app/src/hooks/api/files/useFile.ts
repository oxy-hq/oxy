import { useQuery } from "@tanstack/react-query";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { FileService } from "@/services/api";
import queryKeys from "../queryKey";

export default function useFile(
  pathb64: string,
  enabled = true,
  refetchOnWindowFocus = true,
  refetchOnMount: boolean | "always" = true
) {
  const { project, branchName } = useCurrentProjectBranch();

  return useQuery({
    queryKey: queryKeys.file.get(project.id, branchName, pathb64),
    queryFn: () => FileService.getFile(project.id, pathb64, branchName),
    enabled,
    refetchOnWindowFocus: refetchOnWindowFocus,
    refetchOnMount
  });
}
