use arrow::datatypes::{Schema, SchemaRef};
use arrow::ipc::{reader::FileReader, writer::FileWriter};
use arrow::{array::as_string_array, error::ArrowError, record_batch::RecordBatch};
use arrow_46::datatypes::Schema as Schema64;
use arrow_46::ipc::writer::FileWriter as FileWriter46;
use arrow_46::record_batch::RecordBatch as RecordBatch46;
use connectorx::prelude::{get_arrow, CXQuery, SourceConn};
use duckdb::Connection;
use log::debug;
use std::fs::File;
use std::path::PathBuf;
use std::sync::Arc;
use uuid::Uuid;

use crate::config::model::{ProjectPath, Warehouse, WarehouseType};
use crate::errors::OnyxError;

pub struct Connector {
    config: Warehouse,
}

#[derive(serde::Serialize, Clone)]
pub struct WarehouseInfo {
    name: String,
    dialect: String,
    tables: Vec<String>,
}

impl Connector {
    pub fn new(config: &Warehouse) -> Self {
        Connector {
            config: config.clone(),
        }
    }

    pub async fn load_warehouse_info(&self) -> WarehouseInfo {
        let tables = self.get_schemas().await;
        let name = self.config.dataset.clone();
        let dialect = self.config.warehouse_type.to_string();
        WarehouseInfo {
            name,
            dialect,
            tables,
        }
    }

    pub async fn list_datasets(&self) -> Vec<String> {
        let query_string = match self.config.warehouse_type {
            WarehouseType::Bigquery(_) => {
                "SELECT schema_name FROM INFORMATION_SCHEMA.SCHEMATA".to_owned()
            }
            WarehouseType::DuckDB(_) => "".to_owned(),
        };
        if query_string.is_empty() {
            vec![]
        } else {
            let (datasets, _) = self.run_query_and_load(&query_string).await.unwrap();
            let result_iter = datasets
                .iter()
                .flat_map(|batch| as_string_array(batch.column(0)).iter());
            // datesets
            result_iter
                .map(|name| name.map(|s| s.to_string()))
                .collect::<Option<Vec<String>>>()
                .unwrap_or_default()
        }
    }

    pub async fn get_schemas(&self) -> Vec<String> {
        let query_string = match self.config.warehouse_type {
            WarehouseType::Bigquery(_) => format!(
                "SELECT ddl FROM `{}`.INFORMATION_SCHEMA.TABLES",
                self.config.dataset
            )
            .to_owned(),
            WarehouseType::DuckDB(_) => "".to_owned(),
            _ => "".to_owned(),
        };
        if query_string.is_empty() {
            vec![]
        } else {
            let (datasets, _) = self.run_query_and_load(&query_string).await.unwrap();
            let result_iter = datasets
                .iter()
                .flat_map(|batch| as_string_array(batch.column(0)).iter());
            // ddls
            result_iter
                .map(|name| name.map(|s| s.to_string()))
                .collect::<Option<Vec<String>>>()
                .unwrap_or_default()
        }
    }

    pub async fn run_query(&self, query: &str) -> anyhow::Result<String> {
        let file_path = match &self.config.warehouse_type {
            WarehouseType::Bigquery(bigquery) => {
                let key_path =
                    ProjectPath::get_path(&bigquery.key_path.as_path().to_string_lossy());
                self.run_connectorx_query(query, key_path).await?
            }
            WarehouseType::DuckDB(_) => self.run_duckdb_query(query).await?,
        };
        Ok(file_path)
    }

    pub async fn run_query_and_load(
        &self,
        query: &str,
    ) -> Result<(Vec<RecordBatch>, SchemaRef), OnyxError> {
        let file_path = self
            .run_query(query)
            .await
            .map_err(|e| OnyxError::RuntimeError(format!("Error running query: {}", e)))?;
        load_result(&file_path)
            .map_err(|e| OnyxError::RuntimeError(format!("Error loading query results: {}", e)))
    }

    async fn run_connectorx_query(&self, query: &str, key_path: PathBuf) -> anyhow::Result<String> {
        let current_dir = std::env::current_dir().expect("Failed to get current directory");
        let key_path = current_dir.join(&key_path);
        let conn_string = format!(
            "{}://{}",
            self.config.warehouse_type,
            key_path.to_str().unwrap()
        );
        let query = query.to_string(); // convert to owned string for closure
        let result = tokio::task::spawn_blocking(move || {
            let source_conn = SourceConn::try_from(conn_string.as_str())?;
            let queries = &[CXQuery::from(query.as_str())];
            let destination =
                get_arrow(&source_conn, None, queries).expect("Run failed at get_arrow.");
            let schema = destination.arrow_schema();
            let result = destination.arrow()?;
            let file_path = format!("/tmp/{}.arrow", Uuid::new_v4());
            write_connectorx_to_ipc(&result, &file_path, &schema)?;
            Ok::<String, anyhow::Error>(file_path)
        })
        .await
        .map_err(|e| anyhow::Error::msg(format!("{}", e)))??;

        Ok(result)
    }

    async fn run_duckdb_query(&self, query: &str) -> anyhow::Result<String> {
        let query = query.to_string();
        let conn = Connection::open_in_memory()?;
        let dir_set_stmt = format!(
            "SET file_search_path = '{}'",
            ProjectPath::get_path(&self.config.dataset).display()
        );
        conn.execute(&dir_set_stmt, [])?;
        let mut stmt = conn.prepare(&query)?;
        let arrow_stream = stmt.query_arrow([])?;
        let schema = arrow_stream.get_schema();
        let arrow_chunks = arrow_stream.collect();
        debug!("Query results: {:?}", arrow_chunks);
        let file_path = format!("/tmp/{}.arrow", Uuid::new_v4());
        write_duckdb_to_ipc(&arrow_chunks, &file_path, &schema).unwrap();
        Ok(file_path)
    }
}

pub fn load_result(file_path: &str) -> anyhow::Result<(Vec<RecordBatch>, SchemaRef)> {
    let file = File::open(file_path).map_err(|_| {
        anyhow::Error::msg("Executed query did not generate a valid output file. If you are using an agent to generate the query, consider giving it a shorter prompt.".to_string())
    })?;
    let file = File::open(file_path)?;
    let reader = FileReader::try_new(file, None)?;
    let schema = reader.schema();
    // Collect results and handle potential errors
    let batches: Result<Vec<RecordBatch>, ArrowError> = reader.collect();
    let batches = batches?;

    // Delete the temporary file
    std::fs::remove_file(file_path)?;

    Ok((batches, schema))
}

fn write_connectorx_to_ipc(
    batches: &Vec<RecordBatch46>,
    file_path: &str,
    schema: &Arc<Schema64>,
) -> anyhow::Result<()> {
    let file = File::create(file_path)?;
    if batches.is_empty() {
        debug!("Warning: query returned no results.");
    }
    debug!("Schema: {:?}", schema);
    let schema_ref = schema.as_ref();
    let mut writer = FileWriter46::try_new(file, schema_ref)?;
    debug!(target: "parquet", "Writing batches to parquet file: {:?}", file_path);
    for batch in batches {
        writer.write(batch)?;
    }
    writer.finish()?;
    Ok(())
}

fn write_duckdb_to_ipc(
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
