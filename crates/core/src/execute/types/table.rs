use std::{
    fmt::{self, Debug, Display, Formatter},
    hash::{Hash, Hasher},
    path::PathBuf,
    sync::Arc,
};

use arrow::{array::RecordBatch, datatypes::Schema};
use minijinja::{
    Value,
    value::{Enumerator, Object, ObjectExt, ObjectRepr},
};
use once_cell::sync::OnceCell;
use parquet::{arrow::ArrowWriter, basic::Compression, file::properties::WriterProperties};
use serde::{Deserialize, Serialize};

use crate::{
    adapters::connector::load_result, db::client::get_state_dir, errors::OxyError,
    utils::truncate_datasets,
};

use super::{
    ReferenceKind,
    output_container::TableData,
    reference::QueryReference,
    utils::{record_batches_to_2d_array, record_batches_to_markdown, record_batches_to_table},
};

#[derive(Clone, Debug)]
struct ArrowTable {
    schema: Arc<Schema>,
    batches: Vec<RecordBatch>,
}

impl Display for ArrowTable {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match record_batches_to_table(&self.batches, &self.schema) {
            Ok(table) => write!(f, "{}", table),
            Err(e) => write!(f, "ArrowTable: {}", e),
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct TableReference {
    pub sql: String,
    pub database_ref: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Table {
    pub reference: Option<TableReference>,
    pub file_path: String,
    #[serde(skip)]
    inner: OnceCell<ArrowTable>,
}

impl Hash for Table {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.file_path.hash(state);
    }
}

impl Table {
    pub fn new(file_path: String) -> Self {
        Table {
            reference: None,
            file_path,
            inner: OnceCell::new(),
        }
    }

    pub fn with_reference(file_path: String, reference: TableReference) -> Self {
        Table {
            reference: Some(reference),
            file_path,
            inner: OnceCell::new(),
        }
    }

    fn get_inner(&self) -> Result<&ArrowTable, OxyError> {
        self.inner.get_or_try_init(|| {
            let file_path = self.file_path.clone();
            let (batches, schema) = load_result(&file_path).map_err(|_| {
                OxyError::RuntimeError("Executed query did not generate a valid output file. If you are using an agent to generate the query, consider giving it a shorter prompt.".to_string())
            })?;
            Ok(ArrowTable { schema, batches })
        })
    }

    pub fn to_markdown(&self) -> Result<String, OxyError> {
        let table = self.get_inner()?;
        Ok(record_batches_to_markdown(&table.batches, &table.schema)
            .map_err(|err| {
                OxyError::RuntimeError(format!("Failed to render table result:\n{}", err))
            })?
            .to_string())
    }

    pub fn to_term(&self) -> Result<String, OxyError> {
        let table = self.get_inner()?;
        Ok(record_batches_to_table(&table.batches, &table.schema)
            .map_err(|err| {
                OxyError::RuntimeError(format!("Failed to render table result:\n{}", err))
            })?
            .to_string())
    }

    pub fn into_reference(self) -> Option<ReferenceKind> {
        self.get_inner().ok()?;
        let table = self.inner.into_inner()?;
        let TableReference { sql, database_ref } = self.reference?;
        let (truncated_results, truncated) = truncate_datasets(table.batches);
        let formatted_results =
            record_batches_to_2d_array(&truncated_results, &table.schema).ok()?;
        Some(ReferenceKind::SqlQuery(QueryReference {
            sql_query: sql,
            database: database_ref,
            result: formatted_results,
            is_result_truncated: truncated,
        }))
    }

    // need to convert the file from arrow to parquet
    // because duckdb wasm in the browser have better support for parquet
    // than arrow.
    pub fn to_data(&self, data_path: &PathBuf) -> Result<TableData, OxyError> {
        let table = self.get_inner()?;
        let state_dir = get_state_dir();
        let batches = &table.batches;
        let file_name = format!("{}.parquet", uuid::Uuid::new_v4());
        let full_file_path: PathBuf = data_path.join(file_name);
        let file = std::fs::File::create(&full_file_path).map_err(|e| {
            OxyError::RuntimeError(format!(
                "Failed to create file {}: {}",
                full_file_path.display(),
                e
            ))
        })?;
        let props = WriterProperties::builder()
            .set_compression(Compression::SNAPPY)
            .build();

        let mut writer = ArrowWriter::try_new(file, table.schema.clone(), Some(props))
            .map_err(|e| OxyError::RuntimeError(format!("Failed to create Arrow writer: {}", e)))?;
        for batch in batches {
            writer
                .write(batch)
                .map_err(|e| OxyError::RuntimeError(format!("Failed to write batch: {}", e)))?;
        }
        writer
            .close()
            .map_err(|e| OxyError::RuntimeError(format!("Failed to close writer: {}", e)))?;

        tracing::debug!("Exported table to: {}", full_file_path.display());
        let relative_file_path = full_file_path.strip_prefix(state_dir).map_err(|e| {
            OxyError::RuntimeError(format!("Failed to strip prefix from file path: {}", e))
        })?;

        Ok(TableData {
            file_path: relative_file_path.to_path_buf(),
        })
    }

    pub fn to_export(&self) -> Option<(String, &Arc<Schema>, &Vec<RecordBatch>)> {
        let table = self.get_inner().ok()?;
        let TableReference { sql, .. } = self.reference.clone()?;
        Some((sql, &table.schema, &table.batches))
    }
}

impl Debug for Table {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "Table({:?})", &self.file_path)
    }
}

impl Display for Table {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self.to_markdown() {
            Ok(inner) => {
                write!(f, "{}", inner)
            }
            Err(e) => write!(f, "Table({}): {}", &self.file_path, e),
        }
    }
}

impl Object for Table {
    fn repr(self: &Arc<Self>) -> ObjectRepr {
        ObjectRepr::Iterable
    }

    fn get_value(self: &Arc<Self>, key: &Value) -> Option<Value> {
        let table = self.get_inner().ok()?;
        let schema = table.schema.clone();
        let column_name = key.as_str()?;
        let (idx, _field) = schema.column_with_name(column_name)?;
        let mut values = Vec::new();
        for batch in &table.batches {
            let projected_batch = batch.project(&[idx]).ok()?;
            let json_values: Vec<serde_json::Value> =
                serde_arrow::from_record_batch(&projected_batch).ok()?;
            values.extend(json_values.into_iter().map(|v| v[column_name].clone()));
        }
        tracing::info!("ArrowTable.{} Values: {:?}", key, values);
        Some(Value::from_serialize(values))
    }

    fn enumerate(self: &Arc<Self>) -> Enumerator {
        let table = match self.get_inner() {
            Ok(inner) => inner,
            Err(_) => return Enumerator::Empty,
        };
        let mut values = vec![];

        for record_batch in &table.batches {
            let result: Result<Vec<serde_json::Value>, serde_arrow::Error> =
                serde_arrow::from_record_batch(record_batch);

            match result {
                Ok(json_value) => {
                    for value in json_value {
                        values.push(Value::from_serialize(value));
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to convert record batch to JSON: {}", e);
                    return Enumerator::NonEnumerable;
                }
            }
        }
        Enumerator::Values(values)
    }

    fn render(self: &Arc<Self>, f: &mut fmt::Formatter<'_>) -> fmt::Result
    where
        Self: Sized + 'static,
    {
        match self.repr() {
            ObjectRepr::Seq | ObjectRepr::Iterable if self.enumerator_len().is_some() => {
                for value in self.try_iter().into_iter().flatten() {
                    let _ = &std::fmt::Debug::fmt(&value, f);
                    f.write_str("\n")?;
                }
                f.write_str("")
            }
            _ => {
                write!(f, "{self:?}")
            }
        }
    }
}
