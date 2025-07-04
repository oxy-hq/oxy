import { useMutation } from "@tanstack/react-query";
import { DatabaseService } from "@/services/api";
import useDatabaseOperation from "@/stores/useDatabaseOperation";

export function useDataBuild() {
  const { setSyncState, handleSyncSuccess, handleSyncError } =
    useDatabaseOperation();

  return useMutation({
    mutationFn: () => DatabaseService.buildDatabase(),
    onMutate: () => {
      setSyncState({
        operation: "build",
        database: null,
      });
    },
    onSuccess: (result) => {
      if (result.success) {
        handleSyncSuccess(
          "embeddings",
          result.message || "Embeddings built successfully",
        );
      } else {
        handleSyncError(
          "embeddings",
          undefined,
          result.message || "Failed to build embeddings",
        );
      }
    },
    onError: (error) => {
      handleSyncError(
        "embeddings",
        error,
        "An error occurred while building embeddings",
      );
    },
  });
}
