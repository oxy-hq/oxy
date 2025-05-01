use std::collections::HashMap;

use itertools::Itertools;
use serde::{Serialize, ser::SerializeStruct};

use crate::theme::StyledText;

pub struct SemanticKey {
    pub database: String,
    pub dataset: String,
}

impl SemanticKey {
    pub fn new(database: String, dataset: String) -> Self {
        SemanticKey { database, dataset }
    }
}

#[derive(Debug, Clone)]
pub struct SyncMetrics {
    pub database_ref: String,
    pub sync_time_secs: f64,
    pub output_files: Vec<String>,
}

impl std::fmt::Display for SyncMetrics {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(
            f,
            "{}\nSync Time(seconds): {}\nOutput files:\n{}",
            format!("Database: {}", self.database_ref).success(),
            self.sync_time_secs,
            self.output_files
                .iter()
                .map(|f| format!("- {}", f))
                .join("\n"),
        )
    }
}

pub struct Dataset {
    pub name: String,
    pub tables: Vec<Table>,
}

pub struct Table {
    pub name: String,
    pub columns: Vec<Column>,
}

pub struct Column {
    pub name: String,
    pub data_type: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DatasetInfo {
    pub dataset: String,
    pub ddl: Option<String>,
    pub semantic_info: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct DatabaseInfo {
    pub name: String,
    pub dialect: String,
    pub datasets: HashMap<String, DatasetInfo>,
}

impl DatabaseInfo {
    pub fn tables(&self) -> Vec<String> {
        self.datasets
            .values()
            .filter_map(|dataset| dataset.ddl.clone())
            .collect()
    }
}

impl Serialize for DatabaseInfo {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut state = serializer.serialize_struct("DatabaseInfo", 4)?;
        state.serialize_field("name", &self.name)?;
        state.serialize_field("dialect", &self.dialect)?;
        state.serialize_field("datasets", &self.datasets)?;
        state.serialize_field("tables", &self.tables())?;
        state.end()
    }
}
