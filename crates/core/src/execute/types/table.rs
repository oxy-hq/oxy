use std::{
    fmt::{self, Debug, Display, Formatter},
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    sync::Arc,
};

use arrow::{
    array::{Array, AsArray, RecordBatch},
    datatypes::{DataType, Schema},
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
    adapters::connector::load_result,
    errors::OxyError,
    execute::types::utils::record_batches_to_json,
    utils::{create_parent_dirs, truncate_datasets},
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
    pub name: String,
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
            name: file_path.clone(),
            reference: None,
            file_path,
            max_display_rows: None,
            inner: OnceCell::new(),
        }
    }

    pub fn slug(&self) -> String {
        slugify::slugify(&self.name, "", "-", None)
    }

    pub fn with_reference(
        file_path: String,
        reference: TableReference,
        name: Option<String>,
        max_display_rows: Option<usize>,
    ) -> Self {
        Table {
            name: name.unwrap_or_else(|| file_path.clone()),
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

    pub fn sample(&self) -> Result<&RecordBatch, OxyError> {
        let table = self.get_inner()?;
        if table.batches.is_empty() {
            return Err(OxyError::RuntimeError(
                "No record batches available for sampling.".to_string(),
            ));
        }
        Ok(&table.batches[0])
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
        let file_name = format!("{}.parquet", uuid::Uuid::new_v4());
        let relative_file_path = relative_data_path.join(file_name);
        let full_file_path = base_path.join(&relative_file_path);
        self.save_data(&full_file_path)?;
        Ok(TableData {
            file_path: relative_file_path,
        })
    }

    pub fn save_data<P: AsRef<Path>>(&self, file_path: P) -> Result<String, OxyError> {
        let table = self.get_inner()?;
        let batches = &table.batches;
        create_parent_dirs(file_path.as_ref())?;
        let file = std::fs::File::create(&file_path).map_err(|e| {
            OxyError::RuntimeError(format!(
                "Failed to create file {}: {}",
                file_path.as_ref().display(),
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

        tracing::debug!("Exported table to: {}", file_path.as_ref().display());

        Ok(file_path.as_ref().to_string_lossy().to_string())
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

    pub fn summary_yml(&self) -> String {
        serde_json::from_str(&self.summary())
            .map(|json_value: serde_json::Value| {
                serde_yaml::to_string(&json_value)
                    .unwrap_or_else(|e| format!("Failed to convert summary to YAML: {}", e))
            })
            .unwrap_or_else(|e| format!("Failed to generate summary YAML: {}", e))
    }

    pub fn summary(&self) -> String {
        // Provide a statistical summary of the table in JSON format
        let table = match self.get_inner() {
            Ok(t) => t,
            Err(e) => {
                return serde_json::json!({
                    "error": format!("Failed to get table data: {}", e)
                })
                .to_string();
            }
        };

        let total_rows: usize = table.batches.iter().map(|b| b.num_rows()).sum();
        let mut columns = Vec::new();

        for (col_idx, field) in table.schema.fields().iter().enumerate() {
            let stats = match field.data_type() {
                DataType::Int8
                | DataType::Int16
                | DataType::Int32
                | DataType::Int64
                | DataType::UInt8
                | DataType::UInt16
                | DataType::UInt32
                | DataType::UInt64
                | DataType::Float32
                | DataType::Float64 => self.compute_numeric_stats_json(col_idx, &table.batches),
                _ => self.compute_categorical_stats_json(col_idx, &table.batches),
            };

            columns.push(serde_json::json!({
                "name": field.name(),
                "dtype": format!("{}", field.data_type()),
                "stats": stats
            }));
        }

        serde_json::json!({
            "type": "table",
            "name": self.name,
            "total_rows": total_rows,
            "total_columns": table.schema.fields().len(),
            "columns": columns
        })
        .to_string()
    }

    fn compute_numeric_stats_json(
        &self,
        col_idx: usize,
        batches: &[RecordBatch],
    ) -> serde_json::Value {
        let mut values: Vec<f64> = Vec::new();
        let mut null_count = 0;

        for batch in batches {
            let column = batch.column(col_idx);
            null_count += column.null_count();

            match column.data_type() {
                DataType::Int8 => {
                    if let Some(arr) = column.as_primitive_opt::<arrow::datatypes::Int8Type>() {
                        values.extend(arr.iter().filter_map(|v| v.map(|x| x as f64)));
                    }
                }
                DataType::Int16 => {
                    if let Some(arr) = column.as_primitive_opt::<arrow::datatypes::Int16Type>() {
                        values.extend(arr.iter().filter_map(|v| v.map(|x| x as f64)));
                    }
                }
                DataType::Int32 => {
                    if let Some(arr) = column.as_primitive_opt::<arrow::datatypes::Int32Type>() {
                        values.extend(arr.iter().filter_map(|v| v.map(|x| x as f64)));
                    }
                }
                DataType::Int64 => {
                    if let Some(arr) = column.as_primitive_opt::<arrow::datatypes::Int64Type>() {
                        values.extend(arr.iter().filter_map(|v| v.map(|x| x as f64)));
                    }
                }
                DataType::UInt8 => {
                    if let Some(arr) = column.as_primitive_opt::<arrow::datatypes::UInt8Type>() {
                        values.extend(arr.iter().filter_map(|v| v.map(|x| x as f64)));
                    }
                }
                DataType::UInt16 => {
                    if let Some(arr) = column.as_primitive_opt::<arrow::datatypes::UInt16Type>() {
                        values.extend(arr.iter().filter_map(|v| v.map(|x| x as f64)));
                    }
                }
                DataType::UInt32 => {
                    if let Some(arr) = column.as_primitive_opt::<arrow::datatypes::UInt32Type>() {
                        values.extend(arr.iter().filter_map(|v| v.map(|x| x as f64)));
                    }
                }
                DataType::UInt64 => {
                    if let Some(arr) = column.as_primitive_opt::<arrow::datatypes::UInt64Type>() {
                        values.extend(arr.iter().filter_map(|v| v.map(|x| x as f64)));
                    }
                }
                DataType::Float32 => {
                    if let Some(arr) = column.as_primitive_opt::<arrow::datatypes::Float32Type>() {
                        values.extend(arr.iter().filter_map(|v| v.map(|x| x as f64)));
                    }
                }
                DataType::Float64 => {
                    if let Some(arr) = column.as_primitive_opt::<arrow::datatypes::Float64Type>() {
                        values.extend(arr.iter().flatten());
                    }
                }
                _ => {}
            }
        }

        if values.is_empty() {
            return serde_json::json!({
                "count": values.len() + null_count,
                "non_null_count": 0,
                "null_count": null_count
            });
        }

        let count = values.len();
        let mean = values.iter().sum::<f64>() / count as f64;

        // Calculate standard deviation
        let variance = values.iter().map(|&x| (x - mean).powi(2)).sum::<f64>() / count as f64;
        let std_dev = variance.sqrt();

        // Sort for percentiles
        values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let min = values.first().copied().unwrap_or(0.0);
        let max = values.last().copied().unwrap_or(0.0);
        let median = if count.is_multiple_of(2) {
            (values[count / 2 - 1] + values[count / 2]) / 2.0
        } else {
            values[count / 2]
        };
        let q1 = values[count / 4];
        let q3 = values[3 * count / 4];

        serde_json::json!({
            "count": count + null_count,
            "non_null_count": count,
            "null_count": null_count,
            "mean": format!("{:.2}", mean),
            "std_dev": format!("{:.2}", std_dev),
            "min": format!("{:.2}", min),
            "q1": format!("{:.2}", q1),
            "median": format!("{:.2}", median),
            "q3": format!("{:.2}", q3),
            "max": format!("{:.2}", max)
        })
    }

    fn compute_categorical_stats_json(
        &self,
        col_idx: usize,
        batches: &[RecordBatch],
    ) -> serde_json::Value {
        use std::collections::HashMap;

        let mut value_counts: HashMap<String, usize> = HashMap::new();
        let mut null_count = 0;
        let mut total_count = 0;

        for batch in batches {
            let column = batch.column(col_idx);
            null_count += column.null_count();
            total_count += column.len();

            match column.data_type() {
                DataType::Utf8 => {
                    if let Some(arr) = column.as_string_opt::<i32>() {
                        for v in arr.iter().flatten() {
                            *value_counts.entry(v.to_string()).or_insert(0) += 1;
                        }
                    }
                }
                DataType::LargeUtf8 => {
                    if let Some(arr) = column.as_string_opt::<i64>() {
                        for v in arr.iter().flatten() {
                            *value_counts.entry(v.to_string()).or_insert(0) += 1;
                        }
                    }
                }
                DataType::Boolean => {
                    if let Some(arr) = column.as_boolean_opt() {
                        for v in arr.iter().flatten() {
                            *value_counts.entry(v.to_string()).or_insert(0) += 1;
                        }
                    }
                }
                _ => {
                    // For other types, convert to string representation
                    for i in 0..column.len() {
                        if !column.is_null(i) {
                            let value = format!("{:?}", column.slice(i, 1));
                            *value_counts.entry(value).or_insert(0) += 1;
                        }
                    }
                }
            }
        }

        let unique_count = value_counts.len();
        let non_null_count = total_count - null_count;

        let mut result = serde_json::json!({
            "count": total_count,
            "non_null_count": non_null_count,
            "null_count": null_count,
            "unique_values": unique_count
        });

        if !value_counts.is_empty() {
            let mut sorted_values: Vec<_> = value_counts.iter().collect();
            sorted_values.sort_by(|a, b| b.1.cmp(a.1));

            if let Some((top_value, top_freq)) = sorted_values.first() {
                let obj = result.as_object_mut().unwrap();
                obj.insert(
                    "most_frequent".to_string(),
                    serde_json::json!({
                        "value": top_value,
                        "count": top_freq,
                        "percentage": format!("{:.1}", (**top_freq as f64 / non_null_count as f64) * 100.0)
                    }),
                );
            }
        }

        result
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
                let table_name = &self.name;
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
