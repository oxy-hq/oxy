import { useMutation } from "@tanstack/react-query";
import { service } from "@/services/service";
import useDatabaseOperation from "@/stores/useDatabaseOperation";

export function useDatabaseSync() {
  const { setSyncState, handleSyncSuccess, handleSyncError } =
    useDatabaseOperation();

  return useMutation({
    mutationFn: ({
      database,
      options,
    }: {
      database?: string;
      options?: { datasets?: string[] };
    }) => service.syncDatabase(database, options),
    onMutate: ({ database, options }) => {
      setSyncState({
        operation: "sync",
        database: database || null,
        datasets: options?.datasets,
      });
    },
    onSuccess: (result, { database }) => {
      const dbName = database || "unknown";
      if (result.success) {
        handleSyncSuccess(dbName, result.message);
      } else {
        handleSyncError(dbName, undefined, result.message);
      }
    },
    onError: (error, { database }) => {
      const dbName = database || "unknown";
      handleSyncError(dbName, error);
    },
  });
}
