use crate::theme::*;
use arrow::datatypes::{DataType, Field, Schema};
use arrow::ipc::{reader::FileReader, writer::FileWriter};
use arrow::{array::as_string_array, error::ArrowError, record_batch::RecordBatch};
use arrow_46::ipc::writer::FileWriter as FileWriter46;
use arrow_46::datatypes::{DataType as DataType64, Field as Field64, Schema as Schema64};
use arrow_46::record_batch::RecordBatch as RecordBatch46;
use connectorx::prelude::{get_arrow, CXQuery, SourceConn};
use duckdb::Connection;
use log::debug;
use std::fs::File;
use std::path::PathBuf;
use std::sync::Arc;
use uuid::Uuid;

use crate::config::model::{Warehouse, WarehouseType};

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
            _ => "".to_owned(),
        };
        if query_string.is_empty() {
            vec![]
        } else {
            let result = self.run_query_and_load(&query_string).await.unwrap();
            let result_iter = result
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
            let result = self.run_query_and_load(&query_string).await.unwrap();
            let result_iter = result
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
                self.run_connectorx_query(query, bigquery.key_path.clone())
                    .await?
            }
            WarehouseType::DuckDB(_) => self.run_duckdb_query(query).await?,
            _ => {
                return Err(anyhow::Error::msg(format!(
                    "Unsupported dialect: {}",
                    self.config.warehouse_type
                )))
            }
        };
        Ok(file_path)
    }

    pub async fn run_query_and_load(&self, query: &str) -> anyhow::Result<Vec<RecordBatch>> {
        let file_path = self.run_query(query).await?;
        load_result(&file_path)
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
            let result = destination.arrow()?;
            let file_path = format!("/tmp/{}.arrow", Uuid::new_v4());
            write_connectorx_to_ipc(&result, &file_path)?;
            Ok::<String, anyhow::Error>(file_path)
        })
        .await
        .map_err(|e| anyhow::Error::msg(format!("{}", e)))??;

        Ok(result)
    }

    async fn run_duckdb_query(&self, query: &str) -> anyhow::Result<String> {
        let query = query.to_string();
        let conn = Connection::open_in_memory()?;
        let dir_set_stmt = format!("SET file_search_path = '{}'", self.config.dataset);
        conn.execute(&dir_set_stmt, [])?;
        let mut stmt = conn.prepare(&query)?;
        let arrow_chunks = stmt.query_arrow([])?.collect();
        debug!("Query results: {:?}", arrow_chunks);
        let file_path = format!("/tmp/{}.arrow", Uuid::new_v4());
        write_duckdb_to_ipc(&arrow_chunks, &file_path).unwrap();
        Ok(file_path)
    }
}

pub fn load_result(file_path: &str) -> anyhow::Result<Vec<RecordBatch>> {
    let file = File::open(file_path)?;
    let reader = FileReader::try_new(file, None)?;

    // Collect results and handle potential errors
    let batches: Result<Vec<RecordBatch>, ArrowError> = reader.collect();
    let batches = batches?;

    // Delete the temporary file
    std::fs::remove_file(file_path)?;

    Ok(batches)
}

fn write_connectorx_to_ipc(batches: &Vec<RecordBatch46>, file_path: &str) -> anyhow::Result<()> {
    let file = File::create(file_path)?;
    let schema = if batches.is_empty() {
        println!(
            "{}",
            "Warning: query returned no results.".warning()
        );
        Arc::new(Schema64::new(vec![Field64::new(
            "dummy",
            DataType64::Int32,
            true,
        )]))
    } else {
        batches[0].schema()
    };
    let schema_ref = schema.as_ref();
    let mut writer = FileWriter46::try_new(file, schema_ref)?;
    debug!(target: "parquet", "Writing batches to parquet file: {:?}", file_path);
    for batch in batches {
        writer.write(batch)?;
    }
    writer.finish()?;
    Ok(())
}

fn write_duckdb_to_ipc(batches: &Vec<RecordBatch>, file_path: &str) -> anyhow::Result<()> {
    let file = File::create(file_path)?;
    let schema = if batches.is_empty() {
        println!(
            "{}",
            "Warning: query returned no results.".warning()
        );
        Arc::new(Schema::new(vec![Field::new(
            "dummy",
            DataType::Int32,
            true,
        )]))
    } else {
        batches[0].schema()
    };
    let schema_ref = schema.as_ref();
    let mut writer = FileWriter::try_new(file, schema_ref)?;
    for batch in batches {
        writer.write(batch)?;
    }
    writer.finish()?;
    Ok(())
}
