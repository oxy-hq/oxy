pub(super) const BIGQUERY_DIALECT: &str = "bigquery";
pub(super) const CREATE_CONN: &str = "Failed to open connection";
pub(super) const CREATE_TEMP_TABLE: &str = "Failed to create temporary table";
pub(super) const EXECUTE_QUERY: &str = "Failed to execute query";
pub(super) const LOAD_RESULT: &str = "Error loading query results";
pub(super) const WRITE_RESULT: &str = "Failed to write result to IPC";
pub(super) const SET_FILE_SEARCH_PATH: &str = "Failed to set file search path";
pub(super) const FAILED_TO_RUN_BLOCKING_TASK: &str = "Failed to run blocking task";

pub(super) const SNOWFLAKE_SESSION_VAR_LIMIT: usize = 256;

// duckdb errors
pub(super) const PREPARE_DUCKDB_STMT: &str = "Failed to prepare DuckDB statement";

// arrow errors
pub(super) const LOAD_ARROW_RESULT: &str = "Failed to load arrow result";
