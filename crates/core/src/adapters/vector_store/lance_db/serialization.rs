use crate::{
    adapters::vector_store::{build_index_key, types::RetrievalItem},
    config::constants::RETRIEVAL_EMBEDDINGS_COLUMN,
    errors::OxyError,
};
use arrow::array::{Array, FixedSizeListArray, Float32Array, RecordBatch, StringArray};
use std::sync::Arc;

pub(super) struct SerializationUtils;

impl SerializationUtils {
    pub(super) fn create_retrieval_record_batch(
        items: &Vec<RetrievalItem>,
        n_dims: usize,
    ) -> Result<arrow::array::RecordBatch, OxyError> {
        let schema = super::schema::SchemaUtils::create_retrieval_schema(n_dims);

        let contents = Arc::new(StringArray::from_iter_values(
            items.iter().map(|it| it.content.clone()),
        ));
        let source_types = Arc::new(StringArray::from_iter_values(
            items.iter().map(|it| it.source_type.clone()),
        ));
        let source_identifiers = Arc::new(StringArray::from_iter_values(
            items.iter().map(|it| it.source_identifier.clone()),
        ));
        // upsert_key built from composite of source_identifier and embedding_content
        let upsert_keys = Arc::new(StringArray::from_iter_values(items.iter().map(|it| {
            build_index_key([it.source_identifier.as_str(), it.embedding_content.as_str()])
        })));
        let embedding_contents = Arc::new(StringArray::from_iter_values(
            items.iter().map(|it| it.embedding_content.clone()),
        ));
        let embeddings_array = Arc::new(FixedSizeListArray::from_iter_primitive::<
            arrow::datatypes::Float32Type,
            _,
            _,
        >(
            items
                .iter()
                .map(|it| Some(it.embedding.iter().map(|&v| Some(v)).collect::<Vec<_>>())),
            n_dims.try_into().unwrap(),
        ));
        let radius_array = Arc::new(Float32Array::from_iter_values(
            items.iter().map(|it| it.radius),
        ));

        let record_batch = arrow::array::RecordBatch::try_new(
            schema.clone(),
            vec![
                contents,
                source_types,
                source_identifiers,
                upsert_keys,
                embedding_contents,
                embeddings_array,
                radius_array,
            ],
        )
        .map_err(|e| {
            OxyError::RuntimeError(format!("Failed to create retrieval RecordBatch: {e:?}"))
        })?;

        Ok(record_batch)
    }

    pub(super) fn deserialize_search_records(
        record_batch: &RecordBatch,
    ) -> Result<Vec<(RetrievalItem, f32)>, OxyError> {
        let num_rows = record_batch.num_rows();

        let content_array = Self::get_string_array(record_batch, "content")?;
        let source_type_array = Self::get_string_array(record_batch, "source_type")?;
        let source_identifier_array = Self::get_string_array(record_batch, "source_identifier")?;
        let embedding_content_array = Self::get_string_array(record_batch, "embedding_content")?;
        let embedding_array =
            Self::get_optional_fixed_size_list_array(record_batch, RETRIEVAL_EMBEDDINGS_COLUMN)
                .ok_or_else(|| OxyError::RuntimeError("Missing embedding column".into()))?;
        let radius_array = Self::get_optional_float32_array(record_batch, "radius")
            .ok_or_else(|| OxyError::RuntimeError("Missing radius column".into()))?;
        let distance_array = Self::get_optional_float32_array(record_batch, "_distance")
            .ok_or_else(|| OxyError::RuntimeError("Missing _distance column".into()))?;

        let mut results = Vec::new();
        for i in 0..num_rows {
            let content = content_array.value(i).to_string();
            let source_type = source_type_array.value(i).to_string();
            let source_identifier = source_identifier_array.value(i).to_string();
            let embedding_content = embedding_content_array.value(i).to_string();

            let embedding = if !embedding_array.is_null(i) {
                let embedding_values = embedding_array.value(i);
                let float_array = embedding_values
                    .as_any()
                    .downcast_ref::<Float32Array>()
                    .ok_or_else(|| {
                        OxyError::RuntimeError("Embedding values are not Float32Array".into())
                    })?;
                let len = float_array.len();
                let mut embedding = Vec::with_capacity(len);
                for j in 0..len {
                    embedding.push(float_array.value(j));
                }
                embedding
            } else {
                vec![]
            };

            let radius = if !radius_array.is_null(i) {
                radius_array.value(i)
            } else {
                return Err(OxyError::RuntimeError("Null radius encountered".into()));
            };

            let distance = if !distance_array.is_null(i) {
                distance_array.value(i)
            } else {
                return Err(OxyError::RuntimeError(format!(
                    "Null distance for inclusion '{source_identifier}:{embedding_content}' - this should not be possible after vector search"
                )));
            };

            let item = RetrievalItem {
                content,
                source_type,
                source_identifier,
                embedding_content,
                embedding,
                radius,
            };
            results.push((item, distance));
        }

        Ok(results)
    }

    fn get_string_array<'a>(
        record_batch: &'a RecordBatch,
        column_name: &str,
    ) -> Result<&'a StringArray, OxyError> {
        record_batch
            .column_by_name(column_name)
            .ok_or_else(|| OxyError::RuntimeError(format!("Missing {column_name} column")))?
            .as_any()
            .downcast_ref::<StringArray>()
            .ok_or_else(|| {
                OxyError::RuntimeError(format!("{column_name} column is not a StringArray"))
            })
    }

    fn get_optional_fixed_size_list_array<'a>(
        record_batch: &'a RecordBatch,
        column_name: &str,
    ) -> Option<&'a FixedSizeListArray> {
        record_batch
            .column_by_name(column_name)
            .and_then(|col| col.as_any().downcast_ref::<FixedSizeListArray>())
    }

    fn get_optional_float32_array<'a>(
        record_batch: &'a RecordBatch,
        column_name: &str,
    ) -> Option<&'a Float32Array> {
        record_batch
            .column_by_name(column_name)
            .and_then(|col| col.as_any().downcast_ref::<Float32Array>())
    }
}
