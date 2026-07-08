import { useMutation, useQueryClient } from "@tanstack/react-query";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { DatabaseService } from "@/services/api";
import queryKeys from "../queryKey";

/**
 * Run a DDL statement (e.g. `DROP TABLE …` / `DROP SCHEMA … CASCADE`) against a
 * database connection via the generic SQL-execute endpoint, then refresh that
 * connection's schema tree so the dropped object disappears from the sidebar.
 * Useful for tearing down test schemas/tables.
 */
export default function useDropDbObject(databaseName: string) {
  const { project, branchName } = useCurrentProjectBranch();
  const projectId = project.id;
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (sql: string) =>
      DatabaseService.executeSqlQuery(projectId, branchName, sql, databaseName),
    onSuccess: () => {
      queryClient.invalidateQueries({
        queryKey: queryKeys.database.schema(projectId, branchName, databaseName)
      });
    }
  });
}
