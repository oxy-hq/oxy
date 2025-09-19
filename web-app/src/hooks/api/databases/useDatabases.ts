import { useQuery } from "@tanstack/react-query";
import { DatabaseInfo } from "@/types/database";

import queryKeys from "../queryKey";
import { DatabaseService } from "@/services/api";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";

export default function useDatabases(
  enabled = true,
  refetchOnWindowFocus = true,
  refetchOnMount: boolean | "always" = false,
) {
  const { project, branchName } = useCurrentProjectBranch();
  const projectId = project.id;
  return useQuery<DatabaseInfo[], Error>({
    queryKey: queryKeys.database.list(projectId, branchName),
    queryFn: () => DatabaseService.listDatabases(projectId, branchName),
    enabled,
    refetchOnWindowFocus: refetchOnWindowFocus,
    refetchOnMount,
  });
}
