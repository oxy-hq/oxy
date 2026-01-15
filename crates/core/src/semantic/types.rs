use std::{collections::HashMap, str::FromStr};

use itertools::Itertools;
use serde::{Serialize, ser::SerializeStruct};

use crate::{config::model::Dimension, theme::StyledText};
use oxy_shared::errors::OxyError;

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
pub struct SemanticTableRef {
    pub database: String,
    pub dataset: String,
    pub table: String,
}

impl SemanticTableRef {
    pub fn table_ref(&self) -> String {
        format!("{}.{}.{}.", self.database, self.dataset, self.table)
    }

    pub fn to_target(&self, dimension: &str) -> String {
        format!(
            "{}.{}.{}.{}",
            self.database, self.dataset, self.table, dimension
        )
    }
}

impl FromStr for SemanticTableRef {
    type Err = OxyError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = s.split('.').collect();
        if parts.len() < 3 {
            return Err(OxyError::SerializerError(format!(
                "Invalid semantic table reference format: '{s}'. Expected format: 'database.dataset.table'"
            )));
        }
        Ok(SemanticTableRef {
            database: parts[0].to_string(),
            dataset: parts[1].to_string(),
            table: parts[2].to_string(),
        })
    }
}

#[derive(Debug, Clone)]
pub enum SyncDimension {
    Created {
        dimensions: Vec<Dimension>,
        src: SemanticTableRef,
    },
    DeletedRef {
        src: SemanticTableRef,
    },
}

#[derive(Debug, Clone)]
pub struct SyncMetrics {
    pub database_ref: String,
    pub sync_time_secs: f64,
    pub output_files: Vec<String>,
    pub deleted_files: Vec<String>,
    pub overwritten_files: Vec<String>,
    pub created_files: Vec<String>,
    pub would_overwrite_files: Vec<String>,
    pub dimensions: Vec<SyncDimension>,
}

#[derive(Debug, Clone)]
pub struct SyncOperationResult {
    pub base_path: String,
    pub deleted_files: Vec<String>,
    pub overwritten_files: Vec<String>,
    pub created_files: Vec<String>,
    pub would_overwrite_files: Vec<String>,
    pub dimensions: Vec<SyncDimension>,
}

impl SyncOperationResult {
    pub fn new(base_path: String) -> Self {
        Self {
            base_path,
            deleted_files: Vec::new(),
            overwritten_files: Vec::new(),
            created_files: Vec::new(),
            would_overwrite_files: Vec::new(),
            dimensions: Vec::new(),
        }
    }

    pub fn with_tracking(
        base_path: String,
        deleted_files: Vec<String>,
        overwritten_files: Vec<String>,
        created_files: Vec<String>,
        would_overwrite_files: Vec<String>,
        dimensions: Vec<SyncDimension>,
    ) -> Self {
        Self {
            base_path,
            deleted_files,
            overwritten_files,
            created_files,
            would_overwrite_files,
            dimensions,
        }
    }

    pub fn was_skipped(&self) -> bool {
        !self.would_overwrite_files.is_empty()
    }

    pub fn was_overwritten(&self) -> bool {
        !self.overwritten_files.is_empty()
    }
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
                .map(|f| format!("- {f}"))
                .join("\n"),
        )?;

        if !self.created_files.is_empty() {
            writeln!(
                f,
                "\n{}\n{}",
                "Created files:".success(),
                self.created_files
                    .iter()
                    .map(|f| format!("- {f}"))
                    .join("\n"),
            )?;
        }

        if !self.would_overwrite_files.is_empty() {
            writeln!(
                f,
                "\n{}\n{}",
                "Skipped files (already exist):".warning(),
                self.would_overwrite_files
                    .iter()
                    .map(|f| format!("- {f}"))
                    .join("\n"),
            )?;
        }

        if !self.overwritten_files.is_empty() {
            writeln!(
                f,
                "\n{}\n{}",
                "Overwritten files:".warning(),
                self.overwritten_files
                    .iter()
                    .map(|f| format!("- {f}"))
                    .join("\n"),
            )?;
        }

        if !self.deleted_files.is_empty() {
            writeln!(
                f,
                "\n{}\n{}",
                "Deleted files (not in output):".warning(),
                self.deleted_files
                    .iter()
                    .map(|f| format!("- {f}"))
                    .join("\n"),
            )?;
        }

        Ok(())
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
