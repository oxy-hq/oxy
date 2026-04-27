import { useQuery } from "@tanstack/react-query";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { DatabaseService } from "@/services/api";
import type { DatabaseSchema } from "@/types/database";
import queryKeys from "../queryKey";

export default function useDatabaseSchema(dbName: string, enabled = true) {
  const { project, branchName } = useCurrentProjectBranch();
  const projectId = project.id;
  return useQuery<DatabaseSchema, Error>({
    queryKey: queryKeys.database.schema(projectId, branchName, dbName),
    queryFn: () => DatabaseService.getDatabaseSchema(projectId, branchName, dbName),
    enabled,
    staleTime: 5 * 60 * 1000,
    refetchOnWindowFocus: false
  });
}
