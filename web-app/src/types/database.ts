export interface SemanticEntity {
  name: string;
  description: string;
  sample: string[];
}

export interface SemanticDimension {
  name: string;
  description?: string;
  synonyms?: string[];
  sample?: string[];
  type?: string;
  is_partition_key?: boolean;
}

export interface SemanticMeasure {
  name: string;
  sql: string;
}

export interface SemanticModels {
  table: string;
  database: string;
  description?: string;
  entities?: SemanticEntity[];
  dimensions?: SemanticDimension[];
  measures?: SemanticMeasure[];
  database_name?: string;
}

export interface DatabaseInfo {
  name: string;
  /** SQL dialect for query execution (`"duckdb"`, `"postgres"`, …). */
  dialect: string;
  /** Raw config type from config.yml (`"airhouse_managed"`, `"duckdb"`, …). Use for icons/labels. */
  db_type: string;
  datasets: Record<string, Record<string, SemanticModels>>;
  synced: boolean;
}

export interface ColumnInfo {
  name: string;
  data_type: string;
}

export interface TableInfo {
  name: string;
  columns: ColumnInfo[];
}

export interface DatabaseSchema {
  tables: TableInfo[];
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

// Database Configuration Types
export interface PostgresConfig {
  host?: string;
  port?: string;
  user?: string;
  password?: string;
  password_var?: string;
  database?: string;
}

export interface RedshiftConfig {
  host?: string;
  port?: string;
  user?: string;
  password?: string;
  password_var?: string;
  database?: string;
}

export interface MysqlConfig {
  host?: string;
  port?: string;
  user?: string;
  password?: string;
  password_var?: string;
  database?: string;
}

export interface ClickHouseConfig {
  host?: string;
  user?: string;
  password?: string;
  password_var?: string;
  database?: string;
}

export interface BigQueryConfig {
  key?: string;
  dataset?: string;
  dry_run_limit?: number;
}

export interface DuckDBConfig {
  file_search_path?: string;
}

export interface SnowflakeConfig {
  account?: string;
  username?: string;
  password?: string;
  password_var?: string;
  warehouse?: string;
  database?: string;
  schema?: string;
  role?: string;
  private_key_path?: string;
  auth_mode?: "password" | "browser" | "private_key";
}

export type DatabaseConfigType =
  | "postgres"
  | "redshift"
  | "mysql"
  | "clickhouse"
  | "bigquery"
  | "duckdb"
  | "snowflake";

export type DatabaseConfigValue =
  | PostgresConfig
  | RedshiftConfig
  | MysqlConfig
  | ClickHouseConfig
  | BigQueryConfig
  | DuckDBConfig
  | SnowflakeConfig;

export interface WarehouseConfig {
  type: DatabaseConfigType;
  name?: string;
  config: DatabaseConfigValue;
}

export interface WarehousesFormData {
  warehouses: WarehouseConfig[];
}

export interface CreateDatabaseConfigResponse {
  success: boolean;
  message: string;
  databases_added: string[];
}

// Test Connection Types
export interface TestDatabaseConnectionRequest {
  warehouse: WarehouseConfig;
}

export interface TestDatabaseConnectionResponse {
  success: boolean;
  message: string;
  connection_time_ms?: number;
  error_details?: string;
}

export type ConnectionTestEvent =
  | {
      type: "progress";
      message: string;
    }
  | {
      type: "browser_auth_required";
      sso_url: string;
      message: string;
      timeout_secs?: number;
    }
  | {
      type: "complete";
      result: TestDatabaseConnectionResponse;
    };

// Schema Inspection Types (lightweight discovery used during onboarding —
// returns just schema/table names + column counts so the user can pick tables
// before the heavyweight column sync runs).
export interface DiscoveredTable {
  name: string;
  column_count: number;
}

export interface DiscoveredSchema {
  schema: string;
  tables: DiscoveredTable[];
}

export interface InspectionResult {
  schemas: DiscoveredSchema[];
  schema_count: number;
  table_count: number;
  elapsed_ms: number;
}

export type InspectEvent =
  | { type: "progress"; message: string }
  | { type: "complete"; result: InspectionResult }
  | { type: "error"; message: string };

// Schema-first discovery: the onboarding picker first asks for schemas + total
// table counts (one cheap query per warehouse), then lazily fetches tables for
// each schema when the user expands it.
export interface SchemaSummary {
  schema: string;
  table_count: number;
}

export interface SchemaListResult {
  schemas: SchemaSummary[];
  elapsed_ms: number;
}

export interface SchemaTablesResult {
  schema: string;
  tables: DiscoveredTable[];
  elapsed_ms: number;
}
