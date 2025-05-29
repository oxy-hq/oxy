export interface DatabaseInfo {
  name: string;
  dialect: string;
  datasets: Record<string, string[]>;
}

export interface DatabaseSyncResponse {
  success: boolean;
  message: string;
  sync_time_secs?: number;
}

export interface DatabaseOperationState {
  operation: "sync" | "build" | null;
  database: string | null;
  datasets?: string[];
}

export interface SyncDatabaseParams {
  database?: string;
  datasets?: string[];
}

export type BuildEmbeddingsParams = Record<string, never>;
