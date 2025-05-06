use std::{collections::HashMap, sync::Arc};

use crate::{
    adapters::connector::Connector,
    config::{
        ConfigManager,
        model::{Database, DatabaseType, Dimension, SemanticModels},
    },
    errors::OxyError,
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
            "SELECT {}, {}, {}, {}, {} FROM {}",
            self.column_names.dataset,
            self.column_names.table,
            self.column_names.column,
            self.column_names.data_type,
            self.column_names.is_partitioning_column,
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
            (Some(dataset), Some(table)) => format!(" WHERE {} AND {}", dataset, table),
            (Some(dataset), None) => format!(" WHERE {}", dataset),
            (None, Some(table)) => format!(" WHERE {}", table),
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
        self.datasets()
            .iter()
            .map(|(dataset, tables)| match self.database_type {
                DatabaseType::Bigquery(_) => {
                    let query = GetSchemaQueryBuilder::default()
                        .with_columns_table(format!("{}.INFORMATION_SCHEMA.COLUMNS", dataset))
                        .with_filter_tables(tables.clone())
                        .build();
                    Ok(query)
                }
                DatabaseType::ClickHouse(_) => {
                    let query = GetSchemaQueryBuilder::default()
                        .with_column_names(
                            ColumnNames::default()
                                .with_dataset("database")
                                .with_table("table")
                                .with_column("name")
                                .with_data_type("type")
                                .with_is_partitioning_column("is_in_partition_key"),
                        )
                        .with_filter_dataset(dataset.to_string())
                        .with_filter_tables(tables.clone())
                        .with_columns_table("system.columns".to_string())
                        .build();
                    Ok(query)
                }
                _ => Err(OxyError::ConfigurationError(
                    "Unsupported database type".to_string(),
                )),
            })
            .collect::<Result<Vec<_>, OxyError>>()
    }
    fn get_ddl_queries(&self) -> Result<Vec<String>, OxyError> {
        self.datasets()
            .iter()
            .map(|(dataset, tables)| match self.database_type {
                DatabaseType::Bigquery(_) => {
                    let query = GetSchemaQueryBuilder::default()
                        .with_tables_table(format!("{}.INFORMATION_SCHEMA.TABLES", dataset))
                        .with_filter_tables(tables.clone())
                        .build_ddl();
                    Ok(query)
                }
                DatabaseType::ClickHouse(_) => {
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
                }
                _ => Err(OxyError::ConfigurationError(
                    "Unsupported database type".to_string(),
                )),
            })
            .collect::<Result<Vec<_>, OxyError>>()
    }
}

pub struct SchemaLoader {
    database: Database,
    connector: Arc<Connector>,
}

#[derive(Debug, Deserialize)]
pub(super) struct SchemaRecord {
    #[serde(alias = "table_schema", alias = "database")]
    dataset: String,
    #[serde(alias = "table")]
    table_name: String,
    #[serde(alias = "name")]
    column_name: String,
    #[serde(alias = "type")]
    data_type: String,
    #[serde(alias = "is_in_partition_key", deserialize_with = "deserialize_bool")]
    is_partitioning_column: bool,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum IsPartitionTypes {
    U8(u8),
    Utf8(String),
}

fn deserialize_bool<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: serde::de::Deserializer<'de>,
{
    let value = IsPartitionTypes::deserialize(deserializer)?;
    match value {
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
    #[serde(alias = "table_schema", alias = "database")]
    dataset: String,
    #[serde(alias = "create_table_query")]
    ddl: String,
}

impl SchemaLoader {
    pub async fn from_database(
        database: &Database,
        config: &ConfigManager,
    ) -> Result<Self, OxyError> {
        let connector = Connector::from_database(&database.name, config, None).await?;
        Ok(SchemaLoader {
            database: database.clone(),
            connector: Arc::new(connector),
        })
    }

    async fn load_schema_records(&self, query: &str) -> Result<Vec<SchemaRecord>, OxyError> {
        let (record_batches, _) = self.connector.run_query_with_limit(query, None).await?;
        let mut results = vec![];
        for record_batch in record_batches {
            let records: Vec<SchemaRecord> = from_record_batch(&record_batch).map_err(|e| {
                OxyError::RuntimeError(format!("Failed to parse schema information: {}", e))
            })?;
            results.extend(records);
        }
        Ok(results)
    }

    async fn load_ddl_records(&self, query: &str) -> Result<Vec<DDLRecord>, OxyError> {
        let (record_batches, _) = self.connector.run_query_with_limit(query, None).await?;
        let mut results = vec![];
        for record_batch in record_batches {
            let records: Vec<DDLRecord> = from_record_batch(&record_batch).map_err(|e| {
                OxyError::RuntimeError(format!("Failed to parse schema information: {}", e))
            })?;
            results.extend(records);
        }
        Ok(results)
    }

    pub async fn load_ddl(&self) -> Result<HashMap<String, String>, OxyError> {
        let queries = self.database.get_ddl_queries()?;
        let datasets = async_stream::stream! {
            for query in queries {
                yield async move {
                    self.load_ddl_records(&query).await
                };
            }
        }
        .buffered(10)
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .try_collect::<Vec<DDLRecord>, Vec<_>, _>()?
        .into_iter()
        .flatten()
        .fold(HashMap::new(), |mut acc, record| {
            let entry: &mut String = acc.entry(record.dataset.clone()).or_default();
            entry.push_str(&record.ddl);
            entry.push('\n');
            acc
        });
        Ok(datasets)
    }

    pub async fn load_schema(
        &self,
    ) -> Result<HashMap<String, HashMap<String, SemanticModels>>, OxyError> {
        let queries = self.database.get_schemas_queries()?;
        let datasets = async_stream::stream! {
            for query in queries {
                yield async move {
                    self.load_schema_records(&query).await
                };
            }
        }
        .buffered(10)
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .try_collect::<Vec<SchemaRecord>, Vec<_>, _>()?
        .into_iter()
        .flatten()
        .fold(HashMap::new(), |mut acc, record| {
            let model: &mut HashMap<String, SemanticModels> =
                acc.entry(record.dataset.clone()).or_default();
            let entry = model
                .entry(record.table_name.clone())
                .or_insert(SemanticModels {
                    database: self.database.name.to_string(),
                    table: format!("{}.{}", record.dataset, record.table_name),
                    description: Default::default(),
                    dimensions: vec![],
                    entities: vec![],
                    measures: vec![],
                });
            entry.dimensions.push(Dimension {
                name: record.column_name.to_string(),
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
        Ok(datasets)
    }
}
