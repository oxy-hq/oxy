use arrow::datatypes::{Schema, SchemaRef};
use arrow::ipc::{reader::FileReader, writer::FileWriter};
use arrow::json::ReaderBuilder;
use arrow::json::reader::infer_json_schema;
use arrow::{array::as_string_array, error::ArrowError, record_batch::RecordBatch};
use clickhouse::Client;
use connectorx::prelude::{CXQuery, SourceConn, get_arrow};
use duckdb::Connection;
use itertools::Itertools;
use log::debug;
use snowflake_api::{QueryResult, SnowflakeApi};
use std::fs::File;
use std::io::Cursor;
use std::sync::Arc;
use uuid::Uuid;

use crate::config::ConfigManager;
use crate::config::model::{
    ClickHouse as ConfigClickHouse, DatabaseType, Snowflake as SnowflakeConfig,
};
use crate::errors::OxyError;

const CREATE_CONN: &str = "Failed to open connection";
const EXECUTE_QUERY: &str = "Failed to execute query";
const LOAD_RESULT: &str = "Error loading query results";
const WRITE_RESULT: &str = "Failed to write result to IPC";
const SET_FILE_SEARCH_PATH: &str = "Failed to set file search path";
const FAILED_TO_RUN_BLOCKING_TASK: &str = "Failed to run blocking task";

// duckdb errors
const PREPARE_DUCKDB_STMT: &str = "Failed to prepare DuckDB statement";

// arrow errors
const LOAD_ARROW_RESULT: &str = "Failed to load arrow result";

fn connector_internal_error(message: &str, e: &impl std::fmt::Display) -> OxyError {
    log::error!("{}: {}", message, e);
    OxyError::DBError(format!("{}: {}", message, e))
}

#[enum_dispatch::enum_dispatch]
trait Engine {
    async fn run_query(&self, query: &str) -> Result<String, OxyError>;
    async fn load_database_info(&self) -> Result<DatabaseInfo, OxyError>;
    async fn run_query_and_load(
        &self,
        query: &str,
    ) -> Result<(Vec<RecordBatch>, SchemaRef), OxyError> {
        let file_path = self.run_query(query).await?;
        load_result(&file_path).map_err(|e| connector_internal_error(LOAD_RESULT, &e))
    }
}

#[enum_dispatch::enum_dispatch(Engine)]
#[derive(Debug)]
enum EngineType {
    DuckDB,
    ConnectorX,
    ClickHouse,
    Snowflake,
}

#[derive(Debug)]
struct DuckDB {
    file_search_path: String,
}

impl Engine for DuckDB {
    async fn run_query(&self, query: &str) -> Result<String, OxyError> {
        let query = query.to_string();
        let conn = Connection::open_in_memory()
            .map_err(|err| connector_internal_error(CREATE_CONN, &err))?;
        let dir_set_stmt = format!("SET file_search_path = '{}'", &self.file_search_path);
        conn.execute(&dir_set_stmt, [])
            .map_err(|err| connector_internal_error(SET_FILE_SEARCH_PATH, &err))?;
        let mut stmt = conn
            .prepare(&query)
            .map_err(|err| connector_internal_error(PREPARE_DUCKDB_STMT, &err))?;
        let arrow_stream = stmt
            .query_arrow([])
            .map_err(|err| connector_internal_error(EXECUTE_QUERY, &err))?;
        let schema = arrow_stream.get_schema();
        let arrow_chunks = arrow_stream.collect();
        debug!("Query results: {:?}", arrow_chunks);
        let file_path = format!("/tmp/{}.arrow", Uuid::new_v4());
        write_to_ipc(&arrow_chunks, &file_path, &schema)
            .map_err(|err| connector_internal_error(WRITE_RESULT, &err))?;
        Ok(file_path)
    }

    async fn load_database_info(&self) -> Result<DatabaseInfo, OxyError> {
        Ok(DatabaseInfo {
            name: self.file_search_path.to_string(),
            dialect: "duckdb".to_string(),
            tables: vec![],
        })
    }
}

#[derive(Debug)]
pub struct ConnectorX {
    dialect: String,
    db_path: String,
    db_name: String,
}

impl ConnectorX {
    pub async fn get_schemas(&self) -> Result<Vec<String>, OxyError> {
        let query_string = match self.dialect.as_str() {
            "bigquery" => {
                format!(
                    "SELECT ddl FROM `{}`.INFORMATION_SCHEMA.TABLES",
                    self.db_name
                )
            }
            _ => Err(OxyError::DBError(format!(
                "Unsupported dialect: {}",
                self.dialect
            )))?,
        };
        let (datasets, _) = self.run_query_and_load(&query_string).await?;
        let result_iter = datasets
            .iter()
            .flat_map(|batch| as_string_array(batch.column(0)).iter());
        Ok(result_iter
            .map(|name| name.map(|s| s.to_string()))
            .collect::<Option<Vec<String>>>()
            .unwrap_or_default())
    }
}

impl Engine for ConnectorX {
    async fn run_query(&self, query: &str) -> Result<String, OxyError> {
        let conn_string = format!("{}://{}", self.dialect, self.db_path);
        let query = query.to_string();
        let result = tokio::task::spawn_blocking(move || {
            let source_conn = SourceConn::try_from(conn_string.as_str())
                .map_err(|err| connector_internal_error(CREATE_CONN, &err))?;
            let queries = &[CXQuery::from(query.as_str())];
            let destination = get_arrow(&source_conn, None, queries, None)
                .map_err(|err| connector_internal_error(EXECUTE_QUERY, &err))?;
            let schema = destination.arrow_schema();
            let file_path = format!("/tmp/{}.arrow", Uuid::new_v4());
            let result = destination
                .arrow()
                .map_err(|err| connector_internal_error(LOAD_ARROW_RESULT, &err))?;

            write_to_ipc(&result, &file_path, &schema)
                .map_err(|err| connector_internal_error(WRITE_RESULT, &err))?;
            Ok::<String, anyhow::Error>(file_path)
        })
        .await
        .map_err(|e| connector_internal_error(FAILED_TO_RUN_BLOCKING_TASK, &e))??;

        Ok(result)
    }

    async fn load_database_info(&self) -> Result<DatabaseInfo, OxyError> {
        Ok(DatabaseInfo {
            name: self.db_name.to_string(),
            dialect: self.dialect.to_string(),
            tables: self.get_schemas().await?,
        })
    }
}

#[derive(Debug)]
pub struct ClickHouse {
    pub config: ConfigClickHouse,
}

impl ClickHouse {
    pub async fn get_schemas(&self) -> Result<Vec<String>, OxyError> {
        let query_string = "SELECT name FROM system.tables WHERE database = currentDatabase()";
        let (datasets, _) = self.run_query_and_load(query_string).await?;
        let result_iter = datasets
            .iter()
            .flat_map(|batch| as_string_array(batch.column(0)).iter());
        Ok(result_iter
            .map(|name| name.map(|s| s.to_string()))
            .collect::<Option<Vec<String>>>()
            .unwrap_or_default())
    }
}

impl Engine for ClickHouse {
    async fn run_query(&self, query: &str) -> Result<String, OxyError> {
        let client = Client::default()
            .with_url(self.config.host.clone())
            .with_user(self.config.user.clone())
            .with_password(self.config.get_password().unwrap_or_default())
            .with_database(self.config.database.clone());

        let mut cursor = client.query(query).fetch_bytes("arrow").unwrap();
        let chunks = cursor.collect().await;
        match chunks {
            Ok(chunks) => {
                let cursor = Cursor::new(chunks);
                let reader = FileReader::try_new(cursor, None).unwrap();
                let batches: Vec<RecordBatch> = reader
                    .map(|result| result.map_err(|e| connector_internal_error(LOAD_RESULT, &e)))
                    .collect::<Result<_, _>>()?;

                let schema = batches[0].schema();

                let file_path = format!("/tmp/{}.arrow", Uuid::new_v4());
                write_to_ipc(&batches, &file_path, &schema)
                    .map_err(|err| connector_internal_error(WRITE_RESULT, &err))?;
                Ok(file_path)
            }
            Err(e) => Err(OxyError::DBError(format!("Error fetching data: {}", e)))?,
        }
    }

    async fn load_database_info(&self) -> Result<DatabaseInfo, OxyError> {
        Ok(DatabaseInfo {
            name: self.config.database.clone(),
            dialect: "clickhouse".to_string(),
            tables: self.get_schemas().await?,
        })
    }
}

#[derive(Debug)]
pub struct Snowflake {
    pub config: SnowflakeConfig,
}

impl Snowflake {
    pub async fn get_schemas(&self) -> Result<Vec<String>, OxyError> {
        let query_string = format!(
            "select get_ddl('DATABASE', '{}', false);",
            self.config.database
        );
        let (datasets, _) = self.run_query_and_load(query_string.as_str()).await?;
        let ddl = datasets
            .iter()
            .flat_map(|batch| as_string_array(batch.column(0)).iter())
            .collect::<Vec<_>>()[0]
            .unwrap_or_default();
        let statements = ddl.split(";");
        let mut tables = vec![];
        for statement in statements {
            if statement.contains("create or replace TABLE") {
                tables.push(statement.replace("create or replace TABLE", "create_table"))
            }
        }
        Ok(tables)
    }
}

impl Engine for Snowflake {
    async fn run_query(&self, query: &str) -> Result<String, OxyError> {
        let config = self.config.clone();
        let api = SnowflakeApi::with_password_auth(
            config.account.as_str(),
            Some(config.warehouse.as_str()),
            Some(config.database.as_str()),
            None,
            &config.username,
            config.role.as_deref(),
            &config.get_password().unwrap_or("".to_string()),
        )
        .map_err(|err| connector_internal_error(CREATE_CONN, &err))?;
        let res = api
            .exec(query)
            .await
            .map_err(|err| connector_internal_error(EXECUTE_QUERY, &err))?;
        let record_batches: Vec<RecordBatch>;
        match res {
            QueryResult::Arrow(batches) => {
                record_batches = batches;
            }
            QueryResult::Json(json) => {
                let batches = convert_json_result_to_arrow(&json)?;
                record_batches = batches;
            }
            QueryResult::Empty => return Ok("".to_string()),
        }
        let schema = record_batches[0].schema();
        let file_path = format!("/tmp/{}.arrow", Uuid::new_v4());
        write_to_ipc(&record_batches, &file_path, &schema)
            .map_err(|err| connector_internal_error(WRITE_RESULT, &err))?;
        Ok(file_path)
    }

    async fn load_database_info(&self) -> Result<DatabaseInfo, OxyError> {
        Ok(DatabaseInfo {
            name: self.config.warehouse.clone(),
            dialect: "clickhouse".to_string(),
            tables: self.get_schemas().await?,
        })
    }
}

#[derive(Debug)]
pub struct Connector {
    engine: EngineType,
    pub database_ref: String,
}

#[derive(serde::Serialize, Clone)]
pub struct DatabaseInfo {
    name: String,
    dialect: String,
    tables: Vec<String>,
}

impl Connector {
    pub async fn from_database(
        database_ref: &str,
        config_manager: &ConfigManager,
    ) -> Result<Self, OxyError> {
        let database = config_manager.resolve_database(database_ref)?;
        let engine = match &database.database_type {
            DatabaseType::Bigquery(bigquery) => {
                let key_path = config_manager
                    .resolve_file(
                        bigquery
                            .key_path
                            .as_ref()
                            .ok_or(OxyError::DBError("Key path not set".to_string()))?,
                    )
                    .await?;
                EngineType::ConnectorX(ConnectorX {
                    dialect: database.dialect(),
                    db_path: key_path,
                    db_name: bigquery.dataset.clone(),
                })
            }
            DatabaseType::DuckDB(duckdb) => EngineType::DuckDB(DuckDB {
                file_search_path: config_manager
                    .resolve_file(&duckdb.file_search_path)
                    .await?,
            }),
            DatabaseType::Postgres(pg) => {
                let db_name = pg.database.clone().unwrap_or_default();
                let db_path = format!(
                    "{}:{}@{}:{}/{}",
                    pg.user.clone().unwrap_or_default(),
                    pg.get_password().unwrap_or_default(),
                    pg.host.clone().unwrap_or_default(),
                    pg.port.clone().unwrap_or_default(),
                    db_name,
                );
                EngineType::ConnectorX(ConnectorX {
                    dialect: database.dialect(),
                    db_path,
                    db_name,
                })
            }
            DatabaseType::Redshift(rs) => {
                let db_name = rs.database.clone().unwrap_or_default();
                let db_path = format!(
                    "{}:{}@{}:{}/{}",
                    rs.user.clone().unwrap_or_default(),
                    rs.get_password().unwrap_or_default(),
                    rs.host.clone().unwrap_or_default(),
                    rs.port.clone().unwrap_or_default(),
                    db_name
                );
                EngineType::ConnectorX(ConnectorX {
                    dialect: database.dialect(),
                    db_path,
                    db_name,
                })
            }
            DatabaseType::Mysql(my) => {
                let db_name = my.database.clone().unwrap_or_default();
                let db_path = format!(
                    "{}:{}@{}:{}/{}",
                    my.user.clone().unwrap_or_default(),
                    my.get_password().unwrap_or_default(),
                    my.host.clone().unwrap_or_default(),
                    my.port.clone().unwrap_or_default(),
                    db_name
                );
                EngineType::ConnectorX(ConnectorX {
                    dialect: database.dialect(),
                    db_path,
                    db_name,
                })
            }
            DatabaseType::ClickHouse(ch) => {
                EngineType::ClickHouse(ClickHouse { config: ch.clone() })
            }
            DatabaseType::Snowflake(snowflake) => EngineType::Snowflake(Snowflake {
                config: snowflake.clone(),
            }),
        };
        Ok(Connector {
            engine,
            database_ref: database_ref.to_string(),
        })
    }

    pub async fn database_info(&self) -> Result<DatabaseInfo, OxyError> {
        self.engine.load_database_info().await
    }

    pub async fn run_query(&self, query: &str) -> Result<String, OxyError> {
        self.engine.run_query(query).await
    }

    pub async fn run_query_and_load(
        &self,
        query: &str,
    ) -> Result<(Vec<RecordBatch>, SchemaRef), OxyError> {
        self.engine.run_query_and_load(query).await
    }
}

pub fn load_result(file_path: &str) -> anyhow::Result<(Vec<RecordBatch>, SchemaRef)> {
    let file = File::open(file_path).map_err(|_| {
        anyhow::Error::msg("Executed query did not generate a valid output file. If you are using an agent to generate the query, consider giving it a shorter prompt.".to_string())
    })?;
    let reader = FileReader::try_new(file, None)?;
    let schema = reader.schema();
    // Collect results and handle potential errors
    let batches: Result<Vec<RecordBatch>, ArrowError> = reader.collect();
    let batches = batches?;

    Ok((batches, schema))
}

fn write_to_ipc(
    batches: &Vec<RecordBatch>,
    file_path: &str,
    schema: &Arc<Schema>,
) -> anyhow::Result<()> {
    let file = File::create(file_path)?;
    if batches.is_empty() {
        debug!("Warning: query returned no results.");
    }

    debug!("Schema: {:?}", schema);
    let schema_ref = schema.as_ref();
    let mut writer = FileWriter::try_new(file, schema_ref)?;
    for batch in batches {
        writer.write(batch)?;
    }
    writer.finish()?;
    Ok(())
}

fn convert_json_result_to_arrow(
    json: &snowflake_api::JsonResult,
) -> Result<Vec<RecordBatch>, OxyError> {
    let json_objects = convert_to_json_objects(json);
    let infer_cursor = std::io::Cursor::new(json_objects[0].to_string());
    let (arrow_schema, _) = infer_json_schema(infer_cursor, None)
        .map_err(|err| OxyError::DBError(format!("Failed to infer JSON schema: {}", err)))?;

    let json_string = json_objects.to_string();
    let json_stream_string = json_string[1..json_string.len() - 1]
        .to_string()
        .split(",")
        .join("");
    let cursor = std::io::Cursor::new(json_stream_string);
    let reader = ReaderBuilder::new(Arc::new(arrow_schema))
        .build(cursor)
        .map_err(|err| OxyError::DBError(format!("Failed to create JSON reader: {}", err)))?;
    reader
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| OxyError::DBError(format!("Failed to convert JSON to Arrow: {}", err)))
}

fn convert_to_json_objects(json: &snowflake_api::JsonResult) -> serde_json::Value {
    let mut rs: Vec<serde_json::Value> = vec![];
    if let serde_json::Value::Array(values) = &json.value {
        for value in values {
            let mut m: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();
            if let serde_json::Value::Array(inner_values) = value {
                for field in &json.schema {
                    let field_name = field.name.clone();
                    let field_index = json
                        .schema
                        .iter()
                        .position(|x| x.name == field_name)
                        .unwrap();
                    let field_value = inner_values[field_index].clone();
                    m.insert(field_name, field_value);
                }
            }
            rs.push(serde_json::Value::Object(m));
        }
    }
    serde_json::Value::Array(rs)
}
