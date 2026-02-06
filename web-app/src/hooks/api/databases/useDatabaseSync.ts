import { useMutation, useQueryClient } from "@tanstack/react-query";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { DatabaseService } from "@/services/api";
import useDatabaseOperation from "@/stores/useDatabaseOperation";
import queryKeys from "../queryKey";

export function useDatabaseSync() {
  const { project, branchName } = useCurrentProjectBranch();
  const queryClient = useQueryClient();
  const projectId = project.id;
  const { setSyncState, handleSyncSuccess, handleSyncError } = useDatabaseOperation();
  return useMutation({
    mutationFn: ({ database, options }: { database?: string; options?: { datasets?: string[] } }) =>
      DatabaseService.syncDatabase(projectId, branchName, database, options),
    onMutate: ({ database, options }) => {
      setSyncState({
        operation: "sync",
        database: database || null,
        datasets: options?.datasets
      });
    },
    onSuccess: (result, { database }) => {
      const dbName = database || "unknown";
      if (result.success) {
        handleSyncSuccess(dbName, result.message);
        queryClient.invalidateQueries({ queryKey: queryKeys.database.list(projectId, branchName) });
      } else {
        handleSyncError(dbName, undefined, result.message);
      }
    },
    onError: (error, { database }) => {
      const dbName = database || "unknown";
      handleSyncError(dbName, error);
    }
  });
}
