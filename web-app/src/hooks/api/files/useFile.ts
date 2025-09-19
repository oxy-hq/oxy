import { useQuery } from "@tanstack/react-query";

import queryKeys from "../queryKey";
import { FileService } from "@/services/api";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";

export default function useFile(
  pathb64: string,
  enabled = true,
  refetchOnWindowFocus = true,
  refetchOnMount: boolean | "always" = true,
) {
  const { project, branchName } = useCurrentProjectBranch();

  return useQuery({
    queryKey: queryKeys.file.get(project.id, branchName, pathb64),
    queryFn: () => FileService.getFile(project.id, pathb64, branchName),
    enabled,
    refetchOnWindowFocus: refetchOnWindowFocus,
    refetchOnMount,
  });
}
