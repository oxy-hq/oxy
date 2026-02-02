import React from "react";
import { toast } from "sonner";
import { DatabaseService } from "@/services/api";

interface UseDatabaseSyncParams {
  projectId: string;
  branchName: string;
  refetch: () => Promise<unknown>;
}

export const useDatabaseSync = ({ projectId, branchName, refetch }: UseDatabaseSyncParams) => {
  const [syncingDatabase, setSyncingDatabase] = React.useState<string | null>(null);
  const [syncErrors, setSyncErrors] = React.useState<Record<string, string>>({});

  const handleSyncDatabase = React.useCallback(
    async (e: React.MouseEvent, databaseName: string) => {
      e.preventDefault();
      e.stopPropagation();
      setSyncingDatabase(databaseName);

      setSyncErrors((prev) => {
        const next = { ...prev };
        delete next[databaseName];
        return next;
      });

      try {
        const response = await DatabaseService.syncDatabase(projectId, branchName, databaseName);
        if (response.success) {
          toast.success(response.message || "Database synced successfully");
          await refetch();
        } else {
          const errorMsg = response.message || "Failed to sync database";
          setSyncErrors((prev) => ({ ...prev, [databaseName]: errorMsg }));
          toast.error(errorMsg);
        }
      } catch (error) {
        const message = error instanceof Error ? error.message : "Sync failed";
        setSyncErrors((prev) => ({ ...prev, [databaseName]: message }));
        toast.error(message);
      } finally {
        setSyncingDatabase(null);
      }
    },
    [projectId, branchName, refetch],
  );

  return { syncingDatabase, syncErrors, handleSyncDatabase };
};
