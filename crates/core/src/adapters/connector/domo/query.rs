use std::error::Error;

use crate::{
    adapters::connector::{
        DOMO,
        domo::types::{ColumnMetadata, ExecuteQueryRequest, ExecuteQueryResponse},
    },
    errors::OxyError,
};

#[derive(Debug)]
pub struct DOMOQuery<'a> {
    domo: &'a DOMO,
    base_path: String,
}

impl<'a> DOMOQuery<'a> {
    pub fn new(domo: &'a DOMO, dataset_id: &str) -> Self {
        DOMOQuery {
            domo,
            base_path: format!("/query/v1/execute/{dataset_id}"),
        }
    }

    pub async fn execute(
        &self,
        sql: &ExecuteQueryRequest,
    ) -> Result<ExecuteQueryResponse, OxyError> {
        let response = self
            .domo
            .post(&self.base_path)
            .json(sql)
            .send()
            .await
            .map_err(|err| OxyError::RuntimeError(format!("Failed to send request: {}", err)))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(OxyError::RuntimeError(format!(
                "DOMO API request failed with status {}: {}",
                status, text
            )));
        }
        let json = response.json().await.map_err(|err| {
            OxyError::RuntimeError(format!("Failed to parse json response: {:?}", err.source()))
        })?;
        Ok(json)
    }

    fn infer_schema(
        &self,
        columns: &[String],
        metadata: &[ColumnMetadata],
    ) -> arrow::datatypes::Schema {
        let fields = columns
            .iter()
            .enumerate()
            .map(|(i, col_name)| {
                let col_meta = &metadata[i];
                let data_type = match col_meta.r#type.as_str() {
                    "STRING" => arrow::datatypes::DataType::Utf8,
                    "LONG" => arrow::datatypes::DataType::Int64,
                    "DOUBLE" => arrow::datatypes::DataType::Float64,
                    "BOOLEAN" => arrow::datatypes::DataType::Boolean,
                    "DATE" => arrow::datatypes::DataType::Date32,
                    "DATETIME" => arrow::datatypes::DataType::Timestamp(
                        arrow::datatypes::TimeUnit::Microsecond,
                        None,
                    ),
                    _ => arrow::datatypes::DataType::Utf8, // Default to Utf8 for unknown types
                };
                arrow::datatypes::Field::new(col_name, data_type, true)
            })
            .collect::<Vec<_>>();
        arrow::datatypes::Schema::new(fields)
    }

    // Convert the ExecuteQueryResponse to Arrow RecordBatches
    // using zero-copy where possible
    pub fn to_record_batches(
        &self,
        response: ExecuteQueryResponse,
    ) -> Result<
        (
            Vec<arrow::record_batch::RecordBatch>,
            arrow::datatypes::SchemaRef,
        ),
        OxyError,
    > {
        let schema = self.infer_schema(&response.columns, &response.metadata);
        let schema_ref = arrow::datatypes::SchemaRef::new(schema);
        let mut arrays: Vec<arrow::array::ArrayRef> = vec![];
        for (i, _col_name) in response.columns.iter().enumerate() {
            let col_meta = &response.metadata[i];
            let values: Vec<serde_json::Value> =
                response.rows.iter().map(|row| row[i].clone()).collect();
            let array: arrow::array::ArrayRef = match col_meta.r#type.as_str() {
                "STRING" => {
                    let string_values: Vec<Option<&str>> =
                        values.iter().map(|v| v.as_str()).collect();
                    let array = arrow::array::StringArray::from(string_values);
                    std::sync::Arc::new(array)
                }
                "LONG" => {
                    let int_values: Vec<Option<i64>> = values.iter().map(|v| v.as_i64()).collect();
                    let array = arrow::array::Int64Array::from(int_values);
                    std::sync::Arc::new(array)
                }
                "DOUBLE" => {
                    let float_values: Vec<Option<f64>> =
                        values.iter().map(|v| v.as_f64()).collect();
                    let array = arrow::array::Float64Array::from(float_values);
                    std::sync::Arc::new(array)
                }
                "BOOLEAN" => {
                    let bool_values: Vec<Option<bool>> =
                        values.iter().map(|v| v.as_bool()).collect();
                    let array = arrow::array::BooleanArray::from(bool_values);
                    std::sync::Arc::new(array)
                }
                "DATE" => {
                    let date_values: Vec<Option<i32>> = values
                        .iter()
                        .map(|v| {
                            v.as_str().and_then(|s| {
                                chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
                                    .ok()
                                    .map(|d| {
                                        (d - chrono::NaiveDate::from_ymd_opt(1970, 1, 1).unwrap())
                                            .num_days()
                                            as i32
                                    })
                            })
                        })
                        .collect();
                    let array = arrow::array::Date32Array::from(date_values);
                    std::sync::Arc::new(array)
                }
                "DATETIME" => {
                    let datetime_values: Vec<Option<i64>> = values
                        .iter()
                        .map(|v| {
                            v.as_str().and_then(|s| {
                                chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S")
                                    .ok()
                                    .map(|dt| dt.and_utc().timestamp_micros())
                            })
                        })
                        .collect();
                    let array = arrow::array::TimestampMicrosecondArray::from(datetime_values);
                    std::sync::Arc::new(array)
                }
                _ => {
                    let string_values: Vec<Option<&str>> =
                        values.iter().map(|v| v.as_str()).collect();
                    let array = arrow::array::StringArray::from(string_values);
                    std::sync::Arc::new(array)
                }
            };
            arrays.push(array);
        }
        let record_batch = arrow::record_batch::RecordBatch::try_new(schema_ref.clone(), arrays)
            .map_err(|err| {
                OxyError::RuntimeError(format!("Failed to create RecordBatch: {}", err))
            })?;
        Ok((vec![record_batch], schema_ref))
    }
}
