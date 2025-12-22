import { useMutation, useQueryClient } from "@tanstack/react-query";
import { DatabaseService } from "@/services/api";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { WarehousesFormData } from "@/types/database";
import queryKeys from "../queryKey";
import { toast } from "sonner";

export function useCreateDatabaseConfig() {
  const { project, branchName } = useCurrentProjectBranch();
  const projectId = project.id;
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (warehouses: WarehousesFormData) =>
      DatabaseService.createDatabaseConfig(projectId, branchName, warehouses),
    onSuccess: (result) => {
      if (result.success) {
        toast.success(result.message, {
          description: `Added: ${result.databases_added.join(", ")}`,
        });
        // Invalidate databases query to refresh the list
        queryClient.invalidateQueries({
          queryKey: queryKeys.database.list(projectId, branchName),
        });
      }
    },
    onError: (error: Error) => {
      toast.error("Failed to create database configuration", {
        description: error.message || "An unexpected error occurred",
      });
    },
  });
}
