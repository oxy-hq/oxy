import { useMutation } from "@tanstack/react-query";
import { DatabaseService } from "@/services/api";
import useDatabaseOperation from "@/stores/useDatabaseOperation";

export function useDataClean() {
  const { setSyncState, handleCleanSuccess, handleCleanError } =
    useDatabaseOperation();

  return useMutation({
    mutationFn: (target?: string) => DatabaseService.cleanData(target),
    onMutate: (target) => {
      setSyncState({
        operation: "clean",
        database: null,
        cleanTarget: target || "all",
      });
    },
    onSuccess: (result) => {
      if (result.success) {
        handleCleanSuccess(result.message);
      } else {
        handleCleanError(undefined, result.message);
      }
    },
    onError: (error) => {
      handleCleanError(error, "An error occurred while cleaning data");
    },
  });
}
