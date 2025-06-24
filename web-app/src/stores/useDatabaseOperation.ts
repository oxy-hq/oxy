import { create } from "zustand";
import { persistNSync } from "persist-and-sync";
import { toast } from "sonner";

export interface DatabaseSyncState {
  operation: "sync" | "build" | null;
  database: string | null;
  datasets?: string[];
}

interface DatabaseOperationState {
  syncState: DatabaseSyncState;
  setSyncState: (state: DatabaseSyncState) => void;
  clearSyncState: () => void;
  isSyncing: (database?: string) => boolean;
  isBuilding: () => boolean;
  handleSyncSuccess: (database: string, message?: string) => void;
  handleSyncError: (
    database: string,
    error?: unknown,
    message?: string,
  ) => void;
}

const defaultSyncState: DatabaseSyncState = {
  operation: null,
  database: null,
  datasets: undefined,
};

const useDatabaseOperationStore = create<DatabaseOperationState>()(
  persistNSync(
    (set, get) => ({
      syncState: defaultSyncState,

      setSyncState: (state: DatabaseSyncState) => {
        set({ syncState: state });
      },

      clearSyncState: () => {
        set({ syncState: defaultSyncState });
      },

      isSyncing: (database?: string) => {
        const { syncState } = get();
        if (syncState.operation !== "sync") return false;
        if (!database) return true;
        return syncState.database === database;
      },

      isBuilding: () => {
        const { syncState } = get();
        return syncState.operation === "build";
      },

      handleSyncSuccess: (database: string, message?: string) => {
        toast.success(message || `Database "${database}" synced successfully`);
        get().clearSyncState();
      },

      handleSyncError: (
        database: string,
        error?: unknown,
        message?: string,
      ) => {
        console.error("Database sync error:", error);
        toast.error(message || `Failed to sync database "${database}"`);
        get().clearSyncState();
      },
    }),
    {
      name: "database-operation-storage",
    },
  ),
);

export default function useDatabaseOperation() {
  return useDatabaseOperationStore();
}
