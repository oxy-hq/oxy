use std::{
    fmt::{self, Debug, Display, Formatter},
    hash::{Hash, Hasher},
    path::PathBuf,
    sync::Arc,
};

use arrow::{
    array::RecordBatch,
    datatypes::Schema,
    json::{StructMode, WriterBuilder, writer::JsonArray},
};
use minijinja::{
    Value,
    value::{Enumerator, Object, ObjectExt, ObjectRepr},
};
use once_cell::sync::OnceCell;
use parquet::{arrow::ArrowWriter, basic::Compression, file::properties::WriterProperties};
use serde::{Deserialize, Serialize};

use crate::{
    adapters::connector::load_result, errors::OxyError,
    execute::types::utils::record_batches_to_json, utils::truncate_datasets,
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
            Ok(table) => write!(f, "{table}"),
            Err(e) => write!(f, "ArrowTable: {e}"),
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Default)]
pub struct TableReference {
    pub sql: String,
    pub database_ref: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Table {
    pub name: Option<String>,
    pub reference: Option<TableReference>,
    pub file_path: String,
    pub max_display_rows: Option<usize>,
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
            name: None,
            reference: None,
            file_path,
            max_display_rows: None,
            inner: OnceCell::new(),
        }
    }

    pub fn with_reference(
        file_path: String,
        reference: TableReference,
        name: Option<String>,
        max_display_rows: Option<usize>,
    ) -> Self {
        Table {
            name,
            reference: Some(reference),
            file_path,
            max_display_rows,
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

    pub fn to_markdown(&self) -> String {
        match self.get_inner() {
            Ok(table) => match record_batches_to_markdown(&table.batches, &table.schema) {
                Ok(markdown) => markdown.to_string(),
                Err(e) => {
                    tracing::error!("Failed to convert table to markdown: {}", e);
                    format!("Table({}): {}", &self.file_path, e)
                }
            },
            Err(e) => {
                tracing::error!("Failed to get inner table: {}", e);
                format!("Table({}): {}", &self.file_path, e)
            }
        }
    }

    pub fn to_term(&self) -> Result<String, OxyError> {
        let table = self.get_inner()?;
        Ok(record_batches_to_table(&table.batches, &table.schema)
            .map_err(|err| {
                OxyError::RuntimeError(format!("Failed to render table result:\n{err}"))
            })?
            .to_string())
    }

    pub fn to_2d_array(&self) -> Result<(Vec<Vec<String>>, bool), OxyError> {
        let table = self.get_inner()?;
        let (truncated_results, truncated) = truncate_datasets(&table.batches, None);
        let table_2d_array = record_batches_to_2d_array(&truncated_results, &table.schema)
            .map_err(|err| {
                OxyError::RuntimeError(format!("Failed to convert table to 2D array: {err}"))
            })?;
        Ok((table_2d_array, truncated))
    }

    pub fn get_database_ref(&self) -> Option<String> {
        self.reference.as_ref().map(|r| r.database_ref.clone())
    }

    pub fn get_sql_query(&self) -> Option<String> {
        self.reference.as_ref().map(|r| r.sql.clone())
    }

    pub fn into_reference(self) -> Option<ReferenceKind> {
        self.get_inner().ok()?;
        let table = self.inner.into_inner()?;
        let TableReference { sql, database_ref } = self.reference?;
        let (truncated_results, truncated) = truncate_datasets(&table.batches, None);
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
    pub fn to_data(
        &self,
        relative_data_path: &PathBuf,
        base_path: &PathBuf,
    ) -> Result<TableData, OxyError> {
        let table = self.get_inner()?;
        let batches = &table.batches;
        let file_name = format!("{}.parquet", uuid::Uuid::new_v4());
        let data_path = base_path.join(relative_data_path);
        let full_file_path: PathBuf = data_path.join(&file_name);
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
            .map_err(|e| OxyError::RuntimeError(format!("Failed to create Arrow writer: {e}")))?;
        for batch in batches {
            writer
                .write(batch)
                .map_err(|e| OxyError::RuntimeError(format!("Failed to write batch: {e}")))?;
        }
        writer
            .close()
            .map_err(|e| OxyError::RuntimeError(format!("Failed to close writer: {e}")))?;

        tracing::debug!("Exported table to: {}", full_file_path.display());

        Ok(TableData {
            file_path: relative_data_path.join(file_name).to_path_buf(),
        })
    }

    pub fn to_export(&self) -> Option<(String, &Arc<Schema>, &Vec<RecordBatch>)> {
        let table = self.get_inner().ok()?;
        let TableReference { sql, .. } = self.reference.clone()?;
        Some((sql, &table.schema, &table.batches))
    }

    pub fn to_json(&self) -> Result<serde_json::Map<String, serde_json::Value>, OxyError> {
        let table = self.get_inner()?;
        let mut json_value = serde_json::Map::new();
        let fields = table
            .schema
            .fields()
            .iter()
            .map(|f| {
                let mut field = serde_json::Map::new();
                field.insert(
                    "name".to_string(),
                    serde_json::Value::String(f.name().to_string()),
                );
                field.insert(
                    "dtype".to_string(),
                    serde_json::Value::String(format!("{}", f.data_type())),
                );
                serde_json::Value::Object(field)
            })
            .collect::<Vec<_>>();
        json_value.insert("schema".to_string(), serde_json::Value::Array(fields));
        json_value.insert(
            "type".to_string(),
            serde_json::Value::String("table".to_string()),
        );
        json_value.insert(
            "row_count".to_string(),
            serde_json::Value::Number(serde_json::Number::from(
                table.batches.iter().map(|b| b.num_rows()).sum::<usize>(),
            )),
        );
        let mut object_output: Vec<u8> = Vec::new();
        let mut writer = WriterBuilder::new()
            .with_struct_mode(StructMode::ListOnly)
            .build::<_, JsonArray>(&mut object_output);
        writer
            .write_batches(&table.batches.iter().collect::<Vec<_>>())
            .map_err(|err| {
                OxyError::RuntimeError(format!("Failed to write JSON batches: {err}"))
            })?;
        writer.finish().map_err(|err| {
            OxyError::RuntimeError(format!("Failed to finish writing JSON batches: {err}"))
        })?;
        json_value.insert(
            "data".to_string(),
            serde_json::from_slice(writer.into_inner().as_slice())?,
        );
        Ok(json_value)
    }
}

impl Debug for Table {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "Table({:?})", &self.file_path)
    }
}

impl Display for Table {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self.get_inner() {
            Ok(inner) => {
                let (truncated_results, truncated) =
                    truncate_datasets(&inner.batches, self.max_display_rows);
                let table_name = if let Some(name) = &self.name {
                    format!("table_name: {name} \n  ")
                } else {
                    "".to_string()
                };
                writeln!(f, "- {table_name}data:\n")?;
                match record_batches_to_json(&truncated_results) {
                    Ok(json) => {
                        writeln!(f, "{:2}{json}", "")?;
                        if truncated {
                            writeln!(f, "{:2}Table results has been truncated.", "")?;
                        }
                        Ok(())
                    }
                    Err(e) => writeln!(f, "{:2}Table({}): {}", "", &self.file_path, e),
                }
            }
            Err(e) => writeln!(f, "{:2}Table({}): {}", "", &self.file_path, e),
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
