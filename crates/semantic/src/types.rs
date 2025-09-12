/// Sync metrics for tracking semantic layer synchronization
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

impl std::fmt::Display for SyncMetrics {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Database: {}", self.database_ref)?;
        writeln!(f, "Sync time: {:.2}s", self.sync_time_secs)?;
        writeln!(f, "Output files: {}", self.output_files.len())?;
        writeln!(f, "Created files: {}", self.created_files.len())?;
        writeln!(f, "Overwritten files: {}", self.overwritten_files.len())?;
        writeln!(f, "Deleted files: {}", self.deleted_files.len())?;
        Ok(())
    }
}

/// Sync dimension tracking
#[derive(Debug, Clone)]
pub enum SyncDimension {
    Created {
        dimensions: Vec<DimensionInfo>,
        src: SemanticTableRef,
    },
    DeletedRef {
        src: SemanticTableRef,
    },
}

/// Dimension information
#[derive(Debug, Clone)]
pub struct DimensionInfo {
    pub name: String,
}

/// Semantic table reference
#[derive(Debug, Clone)]
pub struct SemanticTableRef {
    pub database: String,
    pub dataset: String,
    pub table: String,
}

impl SemanticTableRef {
    pub fn new(database: String, dataset: String, table: String) -> Self {
        Self {
            database,
            dataset,
            table,
        }
    }

    pub fn table_ref(&self) -> String {
        format!("{}.{}.{}", self.database, self.dataset, self.table)
    }

    pub fn to_target(&self, dimension: &str) -> String {
        format!("{}.{}", self.table_ref(), dimension)
    }
}

impl std::str::FromStr for SemanticTableRef {
    type Err = crate::SemanticLayerError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = s.split('.').collect();
        if parts.len() != 3 {
            return Err(crate::SemanticLayerError::ParsingError(format!(
                "Invalid table reference format: {}",
                s
            )));
        }
        Ok(Self {
            database: parts[0].to_string(),
            dataset: parts[1].to_string(),
            table: parts[2].to_string(),
        })
    }
}
