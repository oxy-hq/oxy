use std::path::Path;
use std::{collections::HashMap, fs, sync::Arc};

use crate::{
    adapters::connector::Connector,
    config::{
        ConfigManager,
        model::{Database, DatabaseType, Dimension, SemanticModels},
    },
    errors::OxyError,
    utils::extract_csv_dimensions,
};

use futures::StreamExt;
use itertools::Itertools;
use serde::Deserialize;
use serde_arrow::from_record_batch;

pub struct ColumnNames {
    dataset: String,
    table: String,
    column: String,
    data_type: String,
    is_partitioning_column: String,
    ddl: String,
    description: String,
}

impl Default for ColumnNames {
    fn default() -> Self {
        Self {
            dataset: "table_schema".to_string(),
            table: "table_name".to_string(),
            column: "column_name".to_string(),
            data_type: "data_type".to_string(),
            is_partitioning_column: "is_partitioning_column".to_string(),
            ddl: "ddl".to_string(),
            description: "column_comment".to_string(),
        }
    }
}

impl ColumnNames {
    pub fn with_dataset(mut self, dataset: &str) -> Self {
        self.dataset = dataset.to_string();
        self
    }

    pub fn with_table(mut self, table: &str) -> Self {
        self.table = table.to_string();
        self
    }

    pub fn with_column(mut self, column: &str) -> Self {
        self.column = column.to_string();
        self
    }

    pub fn with_data_type(mut self, data_type: &str) -> Self {
        self.data_type = data_type.to_string();
        self
    }

    pub fn with_is_partitioning_column(mut self, is_partitioning_column: &str) -> Self {
        self.is_partitioning_column = is_partitioning_column.to_string();
        self
    }

    pub fn with_ddl(mut self, ddl: &str) -> Self {
        self.ddl = ddl.to_string();
        self
    }

    pub fn with_description(mut self, description: &str) -> Self {
        self.description = description.to_string();
        self
    }
}

pub struct GetSchemaQueryBuilder {
    columns_table: String,
    tables_table: String,
    filter_tables: Vec<String>,
    filter_dataset: Option<String>,
    column_names: ColumnNames,
}

impl Default for GetSchemaQueryBuilder {
    fn default() -> Self {
        Self {
            columns_table: "INFORMATION_SCHEMA.COLUMNS".to_string(),
            tables_table: "INFORMATION_SCHEMA.TABLES".to_string(),
            filter_dataset: None,
            filter_tables: vec![],
            column_names: Default::default(),
        }
    }
}

impl GetSchemaQueryBuilder {
    pub fn with_columns_table(mut self, columns_table: String) -> Self {
        self.columns_table = columns_table;
        self
    }

    pub fn with_tables_table(mut self, tables_table: String) -> Self {
        self.tables_table = tables_table;
        self
    }

    pub fn with_column_names(mut self, column_names: ColumnNames) -> Self {
        self.column_names = column_names;
        self
    }

    pub fn with_filter_dataset(mut self, dataset: String) -> Self {
        if dataset.is_empty() {
            return self;
        }
        self.filter_dataset = Some(dataset);
        self
    }

    pub fn with_filter_tables(mut self, tables: Vec<String>) -> Self {
        self.filter_tables = tables;
        self
    }

    pub fn build_ddl(&self) -> String {
        let mut query = format!(
            "SELECT {}, {} FROM {}",
            self.column_names.dataset, self.column_names.ddl, self.tables_table
        );
        let where_clause = self.get_where_clause();
        query.push_str(&where_clause);
        query
    }

    pub fn build(&self) -> String {
        let mut query = format!(
            "SELECT {}, {}, {}, {}, {}, {} FROM {}",
            self.column_names.dataset,
            self.column_names.table,
            self.column_names.column,
            self.column_names.data_type,
            self.column_names.is_partitioning_column,
            self.column_names.description,
            self.columns_table,
        );
        let where_clause = self.get_where_clause();
        query.push_str(&where_clause);
        query
    }

    fn get_table_filter(&self) -> Option<String> {
        if self.filter_tables.is_empty() {
            return None;
        }
        Some(
            self.filter_tables
                .iter()
                .map(|v| {
                    if v.contains("*") {
                        format!("{} LIKE '{}'", self.column_names.table, v.replace("*", "%"))
                    } else {
                        format!("{} = '{}'", self.column_names.table, v)
                    }
                })
                .join(" OR "),
        )
    }

    fn get_dataset_filter(&self) -> Option<String> {
        self.filter_dataset
            .as_ref()
            .map(|dataset| format!("{} = '{}'", self.column_names.dataset, dataset))
    }

    fn get_where_clause(&self) -> String {
        let dataset_filter = self.get_dataset_filter();
        let table_filter = self.get_table_filter();
        match (dataset_filter, table_filter) {
            (Some(dataset), Some(table)) => format!(" WHERE {dataset} AND {table}"),
            (Some(dataset), None) => format!(" WHERE {dataset}"),
            (None, Some(table)) => format!(" WHERE {table}"),
            (None, None) => String::new(),
        }
    }
}

trait GetSchemaQuery {
    fn get_schemas_queries(&self) -> Result<Vec<String>, OxyError>;
    fn get_ddl_queries(&self) -> Result<Vec<String>, OxyError>;
}

impl GetSchemaQuery for Database {
    fn get_schemas_queries(&self) -> Result<Vec<String>, OxyError> {
        match &self.database_type {
            DatabaseType::Bigquery(_) => self
                .datasets()
                .iter()
                .map(|(dataset, tables)| {
                    let tables_filter = if tables.is_empty() {
                        String::new()
                    } else {
                        let table_conditions = tables
                            .iter()
                            .map(|v| {
                                if v.contains("*") {
                                    format!("c.table_name LIKE '{}'", v.replace("*", "%"))
                                } else {
                                    format!("c.table_name = '{v}'")
                                }
                            })
                            .join(" OR ");
                        format!(" WHERE {table_conditions}")
                    };

                    let query = format!(
                        "SELECT c.table_schema, c.table_name, c.column_name, c.data_type, c.is_partitioning_column, COALESCE(d.description, NULL) as description
                         FROM `{dataset}.INFORMATION_SCHEMA.COLUMNS` c
                         LEFT JOIN `{dataset}.INFORMATION_SCHEMA.COLUMN_FIELD_PATHS` d
                         ON c.table_name = d.table_name AND c.column_name = d.column_name{tables_filter}"
                    );
                    Ok(query)
                })
                .collect::<Result<Vec<_>, OxyError>>(),
            DatabaseType::ClickHouse(_) => self
                .datasets()
                .iter()
                .map(|(dataset, tables)| {
                    let query = GetSchemaQueryBuilder::default()
                        .with_column_names(
                            ColumnNames::default()
                                .with_dataset("database")
                                .with_table("table")
                                .with_column("name")
                                .with_data_type("type")
                                .with_is_partitioning_column("is_in_partition_key")
                                .with_description("comment"),
                        )
                        .with_filter_dataset(dataset.to_string())
                        .with_filter_tables(tables.clone())
                        .with_columns_table("system.columns".to_string())
                        .build();
                    Ok(query)
                })
                .collect::<Result<Vec<_>, OxyError>>(),
            DatabaseType::Snowflake(_) => self
                .datasets()
                .iter()
                .map(|(dataset, tables)| {
                    let tables_filter = if tables.is_empty() {
                        String::new()
                    } else {
                        let table_conditions = tables
                            .iter()
                            .map(|v| {
                                if v.contains("*") {
                                    format!("c.table_name LIKE '{}'", v.replace("*", "%"))
                                } else {
                                    format!("c.table_name = '{v}'")
                                }
                            })
                            .join(" OR ");
                        format!(" AND ({table_conditions})")
                    };

                    let query = format!(
                        "SELECT c.TABLE_SCHEMA as table_schema, 
                                c.TABLE_NAME as table_name, 
                                c.COLUMN_NAME as column_name, 
                                c.DATA_TYPE as data_type,
                                CASE WHEN c.IS_IDENTITY = 'YES' THEN TRUE ELSE FALSE END as is_partitioning_column,
                                c.COMMENT as description
                         FROM INFORMATION_SCHEMA.COLUMNS c
                         WHERE c.TABLE_SCHEMA = '{dataset}'{tables_filter}
                         ORDER BY c.TABLE_NAME, c.ORDINAL_POSITION"
                    );
                    tracing::info!("🔍 Snowflake schema query for dataset '{}': {}", dataset, query);
                    Ok(query)
                })
                .collect::<Result<Vec<_>, OxyError>>(),

            _ => Err(OxyError::ConfigurationError(
                "Unsupported database type".to_string(),
            )),
        }
    }
    fn get_ddl_queries(&self) -> Result<Vec<String>, OxyError> {
        match &self.database_type {
            DatabaseType::Bigquery(_) => self
                .datasets()
                .iter()
                .map(|(dataset, tables)| {
                    let query = GetSchemaQueryBuilder::default()
                        .with_tables_table(format!("{dataset}.INFORMATION_SCHEMA.TABLES"))
                        .with_filter_tables(tables.clone())
                        .build_ddl();
                    Ok(query)
                })
                .collect::<Result<Vec<_>, OxyError>>(),
            DatabaseType::ClickHouse(_) => self
                .datasets()
                .iter()
                .map(|(dataset, tables)| {
                    let query = GetSchemaQueryBuilder::default()
                        .with_column_names(
                            ColumnNames::default()
                                .with_dataset("database")
                                .with_ddl("create_table_query")
                                .with_table("name"),
                        )
                        .with_tables_table("system.tables".to_string())
                        .with_filter_dataset(dataset.to_string())
                        .with_filter_tables(tables.clone())
                        .build_ddl();
                    Ok(query)
                })
                .collect::<Result<Vec<_>, OxyError>>(),
            DatabaseType::Snowflake(_) => {
                // Snowflake's GET_DDL function requires constant arguments and cannot be used
                // in bulk queries with dynamic table names. DDL information is not critical
                // for semantic model generation, so we skip it for Snowflake.
                Ok(vec![])
            },

            _ => Err(OxyError::ConfigurationError(
                "Unsupported database type".to_string(),
            )),
        }
    }
}

async fn fetch_schema_models<T: for<'de> Deserialize<'de>>(
    queries: Vec<String>,
    connector: &Arc<Connector>,
) -> Result<Vec<T>, OxyError> {
    let datasets = async_stream::stream! {
        for (query_idx, query) in queries.into_iter().enumerate() {
            yield async move {
                tracing::info!("🚀 Executing query #{}: {}", query_idx + 1, query);
                let (record_batches, schema) = connector.run_query_with_limit(&query, None).await?;
                tracing::info!("✅ Query #{} completed. Schema: {:?}", query_idx + 1, schema);
                
                let mut results = vec![];
                for (batch_idx, record_batch) in record_batches.iter().enumerate() {
                    tracing::info!("📊 Processing batch #{}: {} rows", batch_idx + 1, record_batch.num_rows());
                    let records: Vec<T> = from_record_batch(record_batch).map_err(|e| {
                        OxyError::RuntimeError(format!("Failed to parse schema information: {e}"))
                    })?;
                    tracing::info!("🔄 Parsed {} records from batch #{}", records.len(), batch_idx + 1);
                    results.extend(records);
                }
                tracing::info!("📈 Total records from query #{}: {}", query_idx + 1, results.len());
                Ok::<_, OxyError>(results)
            };
        }
    }
    .buffered(10)
    .collect::<Vec<_>>()
    .await
    .into_iter()
    .try_collect::<Vec<T>, Vec<_>, _>()?
    .into_iter()
    .flatten()
    .collect();
    Ok(datasets)
}

pub struct SchemaLoader {
    database: Database,
    connector: Arc<Connector>,
}

#[derive(Debug, Deserialize)]
pub(super) struct SchemaRecord {
    #[serde(alias = "table_schema", alias = "database", alias = "TABLE_SCHEMA")]
    dataset: String,
    #[serde(alias = "table", alias = "TABLE_NAME")]
    table_name: String,
    #[serde(alias = "name", alias = "COLUMN_NAME")]
    column_name: String,
    #[serde(alias = "type", alias = "DATA_TYPE")]
    data_type: String,
    #[serde(alias = "is_in_partition_key", alias = "IS_PARTITIONING_COLUMN", deserialize_with = "deserialize_bool")]
    is_partitioning_column: bool,
    #[serde(alias = "column_comment", alias = "comment", alias = "DESCRIPTION")]
    description: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum IsPartitionTypes {
    Bool(bool),
    U8(u8),
    Utf8(String),
}

fn deserialize_bool<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: serde::de::Deserializer<'de>,
{
    let value = IsPartitionTypes::deserialize(deserializer)?;
    match value {
        IsPartitionTypes::Bool(value) => Ok(value),
        IsPartitionTypes::U8(value) => Ok(value > 0),
        IsPartitionTypes::Utf8(value) => match value.to_lowercase().as_str() {
            "yes" => Ok(true),
            "no" => Ok(false),
            _ => Err(serde::de::Error::custom(
                "Expected 'yes', 'no', 'YES' or 'NO'",
            )),
        },
    }
}

#[derive(Debug, Deserialize)]
pub(super) struct DDLRecord {
    #[serde(alias = "table_schema", alias = "database", alias = "TABLE_SCHEMA")]
    dataset: String,
    #[serde(alias = "create_table_query")]
    ddl: String,
}

impl SchemaLoader {
    pub async fn from_database(
        database: &Database,
        config: &ConfigManager,
    ) -> Result<Self, OxyError> {
        let connector = Arc::new(Connector::from_database(&database.name, config, None).await?);
        Ok(SchemaLoader {
            database: database.clone(),
            connector,
        })
    }

    pub async fn load_schema(
        &self,
    ) -> Result<HashMap<String, HashMap<String, SemanticModels>>, OxyError> {
        match &self.database.database_type {
            DatabaseType::DuckDB(duckdb) => {
                let path = Path::new(&duckdb.file_search_path);
                let mut result = HashMap::new();
                let mut tables = HashMap::new();

                for entry in fs::read_dir(path).map_err(|e| {
                    OxyError::RuntimeError(format!("Failed to read DuckDB directory: {e}"))
                })? {
                    let entry = entry.map_err(|e| {
                        OxyError::RuntimeError(format!(
                            "Failed to read DuckDB directory entry: {e}"
                        ))
                    })?;
                    let path = entry.path();
                    let ext = path
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or("")
                        .to_lowercase();
                    let table_name = path.file_name().unwrap().to_string_lossy().to_string();
                    let dimensions = match ext.as_str() {
                        "csv" => extract_csv_dimensions(&path),
                        // "parquet" | "json" => not supported for now
                        _ => Ok(vec![]),
                    }?;
                    if !dimensions.is_empty() {
                        tables.insert(
                            path.file_stem().unwrap().to_string_lossy().to_string(),
                            SemanticModels {
                                database: self.database.name.clone(),
                                table: table_name.clone(),
                                description: "".to_string(),
                                dimensions,
                                entities: vec![],
                                measures: vec![],
                            },
                        );
                    }
                }
                if !tables.is_empty() {
                    result.insert("duckdb".to_string(), tables);
                }
                Ok(result)
            }
            DatabaseType::ClickHouse(_) | DatabaseType::Bigquery(_) | DatabaseType::Snowflake(_) => {
                let db_type = match &self.database.database_type {
                    DatabaseType::ClickHouse(_) => "ClickHouse",
                    DatabaseType::Bigquery(_) => "BigQuery", 
                    DatabaseType::Snowflake(_) => "Snowflake",
                    _ => "Unknown"
                };
                tracing::info!("🏗️  Starting schema load for {} database: {}", db_type, self.database.name);
                eprintln!("🎯 DEBUG: Starting schema load for {} database: {}", db_type, self.database.name);
                
                let queries = self.database.get_schemas_queries()?;
                tracing::info!("📝 Generated {} schema queries for {}", queries.len(), db_type);
                eprintln!("🎯 DEBUG: Generated {} schema queries", queries.len());
                for (i, query) in queries.iter().enumerate() {
                    eprintln!("🎯 DEBUG: Query {}: {}", i + 1, query);
                }
                
                let records: Vec<SchemaRecord> =
                    fetch_schema_models(queries, &self.connector).await?;
                tracing::info!("📋 Retrieved {} schema records from {}", records.len(), db_type);
                eprintln!("🎯 DEBUG: Retrieved {} schema records", records.len());
                let datasets = records.into_iter().fold(HashMap::new(), |mut acc, record| {
                    let model: &mut HashMap<String, SemanticModels> =
                        acc.entry(record.dataset.clone()).or_default();
                    let table_name = match &self.database.database_type {
                        DatabaseType::Snowflake(sf) => {
                            // For Snowflake, use database.schema.table format
                            format!("{}.{}.{}", sf.database, record.dataset, record.table_name)
                        },
                        _ => {
                            // For other databases, use schema.table format
                            format!("{}.{}", record.dataset, record.table_name)
                        }
                    };
                    
                    let entry: &mut SemanticModels = model
                        .entry(record.table_name.clone())
                        .or_insert(SemanticModels {
                            database: self.database.name.to_string(),
                            table: table_name,
                            description: Default::default(),
                            dimensions: vec![],
                            entities: vec![],
                            measures: vec![],
                        });
                    entry.dimensions.push(Dimension {
                        name: record.column_name.to_string(),
                        description: record.description,
                        synonyms: None,
                        sample: vec![],
                        data_type: Some(record.data_type.to_string()),
                        is_partition_key: if record.is_partitioning_column {
                            Some(record.is_partitioning_column)
                        } else {
                            None
                        },
                    });
                    acc
                });
                
                let total_tables = datasets.values().map(|tables| tables.len()).sum::<usize>();
                let total_dimensions = datasets.values()
                    .flat_map(|tables| tables.values())
                    .map(|table| table.dimensions.len())
                    .sum::<usize>();
                
                tracing::info!("🎯 Final results for {} database '{}': {} datasets, {} tables, {} dimensions", 
                    db_type, self.database.name, datasets.len(), total_tables, total_dimensions);
                
                for (dataset_name, tables) in &datasets {
                    tracing::info!("  📁 Dataset '{}': {} tables", dataset_name, tables.len());
                    for (table_name, table) in tables {
                        tracing::info!("    📊 Table '{}': {} dimensions", table_name, table.dimensions.len());
                    }
                }
                
                Ok(datasets)
            }
            _ => Err(OxyError::ConfigurationError(
                "Unsupported database type".to_string(),
            )),
        }
    }

    pub async fn load_ddl(&self) -> Result<HashMap<String, String>, OxyError> {
        match &self.database.database_type {
            DatabaseType::DuckDB(duckdb) => {
                let path = Path::new(&duckdb.file_search_path);
                let mut ddls = HashMap::new();
                let mut ddl_lines = Vec::new();

                use duckdb::Connection;
                let conn = Connection::open_in_memory().map_err(|e| {
                    OxyError::RuntimeError(format!("Failed to open in-memory DuckDB: {e}"))
                })?;

                for entry in fs::read_dir(path).map_err(|e| {
                    OxyError::RuntimeError(format!("Failed to read DuckDB directory: {e}"))
                })? {
                    let entry = entry.map_err(|e| {
                        OxyError::RuntimeError(format!(
                            "Failed to read DuckDB directory entry: {e}"
                        ))
                    })?;
                    let path = entry.path();
                    let ext = path
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or("")
                        .to_lowercase();
                    let file_name = path.file_stem().unwrap().to_string_lossy().to_string();
                    let columns = match ext.as_str() {
                        "csv" => {
                            let sql = format!(
                                "CREATE OR REPLACE VIEW auto_csv AS SELECT * FROM read_csv_auto('{}', SAMPLE_SIZE=10000, ALL_VARCHAR=FALSE);",
                                path.display()
                            );
                            conn.execute(&sql, []).map_err(|e| {
                                OxyError::RuntimeError(format!(
                                    "DuckDB failed to read CSV {}: {}",
                                    path.display(),
                                    e
                                ))
                            })?;

                            let mut stmt =
                                conn.prepare("PRAGMA table_info('auto_csv');")
                                    .map_err(|e| {
                                        OxyError::RuntimeError(format!(
                                            "DuckDB failed to prepare schema query: {e}"
                                        ))
                                    })?;
                            let mut rows = stmt.query([]).map_err(|e| {
                                OxyError::RuntimeError(format!(
                                    "DuckDB failed to query schema: {e}"
                                ))
                            })?;
                            let mut columns: Vec<String> = Vec::new();
                            while let Some(row) = rows.next().map_err(|e| {
                                OxyError::RuntimeError(format!(
                                    "DuckDB failed to read schema row: {e}"
                                ))
                            })? {
                                let name: String = row.get(1).map_err(|e| {
                                    OxyError::RuntimeError(format!("DuckDB schema row: {e}"))
                                })?;
                                let dtype: String = row.get(2).map_err(|e| {
                                    OxyError::RuntimeError(format!("DuckDB schema row: {e}"))
                                })?;
                                columns.push(format!("\"{name}\" {dtype}"));
                            }
                            Ok::<Vec<String>, OxyError>(columns)
                        }
                        // "parquet" | "json" => not supported for now
                        _ => Ok::<Vec<String>, OxyError>(vec![]),
                    }?;
                    if !columns.is_empty() {
                        let ddl =
                            format!("CREATE TABLE '{}' ({});", file_name, columns.join(", "),);
                        ddl_lines.push(format!("-- {file_name}\n{ddl}"));
                    }
                }
                if !ddl_lines.is_empty() {
                    ddls.insert("duckdb".to_string(), ddl_lines.join("\n\n"));
                }
                Ok(ddls)
            }
            DatabaseType::ClickHouse(_) | DatabaseType::Bigquery(_) | DatabaseType::Snowflake(_) => {
                let queries = self.database.get_ddl_queries()?;
                let records: Vec<DDLRecord> = fetch_schema_models(queries, &self.connector).await?;
                let datasets = records.into_iter().fold(HashMap::new(), |mut acc, record| {
                    let entry: &mut String = acc.entry(record.dataset.clone()).or_default();
                    entry.push_str(&record.ddl);
                    entry.push('\n');
                    acc
                });
                Ok(datasets)
            }
            _ => Err(OxyError::ConfigurationError(
                "Unsupported database type".to_string(),
            )),
        }
    }
}
