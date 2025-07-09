use std::sync::Arc;
use arrow::{
    array::{
        Array, RecordBatch, StringArray, ListBuilder, StructBuilder, StringBuilder, 
        FixedSizeListBuilder, Float32Builder, ListArray, StructArray, FixedSizeListArray, Float32Array
    },
};
use crate::{
    config::constants::{RETRIEVAL_DEFAULT_INCLUSION_RADIUS, RETRIEVAL_INCLUSION_MIDPOINT_COLUMN},
    errors::OxyError
};
use super::super::types::{Document, SearchRecord, RetrievalContent};
use super::schema::SchemaUtils;

pub(super) struct SerializationUtils;

impl SerializationUtils {
    fn get_string_array<'a>(record_batch: &'a RecordBatch, column_name: &str) -> Result<&'a StringArray, OxyError> {
        record_batch
            .column_by_name(column_name)
            .ok_or_else(|| OxyError::RuntimeError(format!("Missing {} column", column_name)))?
            .as_any()
            .downcast_ref::<StringArray>()
            .ok_or_else(|| OxyError::RuntimeError(format!("{} column is not a StringArray", column_name)))
    }
    
    fn get_optional_list_array<'a>(record_batch: &'a RecordBatch, column_name: &str) -> Option<&'a ListArray> {
        record_batch
            .column_by_name(column_name)
            .and_then(|col| col.as_any().downcast_ref::<ListArray>())
    }
    
    fn get_optional_fixed_size_list_array<'a>(record_batch: &'a RecordBatch, column_name: &str) -> Option<&'a FixedSizeListArray> {
        record_batch
            .column_by_name(column_name)
            .and_then(|col| col.as_any().downcast_ref::<FixedSizeListArray>())
    }
    
    fn get_optional_float32_array<'a>(record_batch: &'a RecordBatch, column_name: &str) -> Option<&'a Float32Array> {
        record_batch
            .column_by_name(column_name)
            .and_then(|col| col.as_any().downcast_ref::<Float32Array>())
    }
    
    fn get_struct_field_as_string_array<'a>(struct_array: &'a StructArray, field_name: &str) -> Result<&'a StringArray, OxyError> {
        struct_array
            .column_by_name(field_name)
            .ok_or_else(|| OxyError::RuntimeError(format!("Missing {} field", field_name)))?
            .as_any()
            .downcast_ref::<StringArray>()
            .ok_or_else(|| OxyError::RuntimeError(format!("{} is not a StringArray", field_name)))
    }
    
    fn get_struct_field_as_fixed_size_list_array<'a>(struct_array: &'a StructArray, field_name: &str) -> Result<&'a FixedSizeListArray, OxyError> {
        struct_array
            .column_by_name(field_name)
            .ok_or_else(|| OxyError::RuntimeError(format!("Missing {} field", field_name)))?
            .as_any()
            .downcast_ref::<FixedSizeListArray>()
            .ok_or_else(|| OxyError::RuntimeError(format!("{} is not a FixedSizeListArray", field_name)))
    }

    pub(super) fn create_retrieval_content_array(
        retrieval_contents: &Vec<Vec<RetrievalContent>>,
        n_dims: usize,
    ) -> anyhow::Result<Arc<dyn Array>> {
        let struct_fields = SchemaUtils::create_retrieval_content_fields(n_dims);
        let string_builder = StringBuilder::new();
        let embeddings_builder = FixedSizeListBuilder::new(
            Float32Builder::new(),
            n_dims.try_into().unwrap(),
        );
        let struct_builder = StructBuilder::new(
            struct_fields.clone(),
            vec![
                Box::new(string_builder) as Box<dyn arrow::array::ArrayBuilder>,
                Box::new(embeddings_builder) as Box<dyn arrow::array::ArrayBuilder>,
            ],
        );
        
        let list_field = arrow::datatypes::Field::new(
            "item",
            SchemaUtils::create_retrieval_content_struct_type(n_dims),
            false,
        );
        let mut list_builder = ListBuilder::new(struct_builder).with_field(list_field);
        
        for document_contents in retrieval_contents {
            for content in document_contents {
                list_builder
                    .values()
                    .field_builder::<StringBuilder>(0)
                    .unwrap()
                    .append_value(&content.embedding_content);
                
                let embeddings_field_builder = list_builder
                    .values()
                    .field_builder::<FixedSizeListBuilder<Float32Builder>>(1)
                    .unwrap();
                
                for &embedding_val in &content.embeddings {
                    embeddings_field_builder.values().append_value(embedding_val);
                }
                embeddings_field_builder.append(true);                
                list_builder.values().append(true);
            }
            list_builder.append(true);
        }
        
        Ok(Arc::new(list_builder.finish()))
    }

    pub(super) fn deserialize_search_records(record_batch: &RecordBatch) -> Result<Vec<SearchRecord>, OxyError> {
        let num_rows = record_batch.num_rows();

        let content_array = Self::get_string_array(record_batch, "content")?;
        let source_type_array = Self::get_string_array(record_batch, "source_type")?;
        let source_identifier_array = Self::get_string_array(record_batch, "source_identifier")?;
        let inclusions_array = Self::get_optional_list_array(record_batch, "retrieval_inclusions");
        let exclusions_array = Self::get_optional_list_array(record_batch, "retrieval_exclusions");
        let midpoint_array = Self::get_optional_fixed_size_list_array(record_batch, "inclusion_midpoint");
        let radius_array = Self::get_optional_float32_array(record_batch, "inclusion_radius");
        let distance_array = Self::get_optional_float32_array(record_batch, "_distance");
        let score_array = Self::get_optional_float32_array(record_batch, "_score")
            .or_else(|| Self::get_optional_float32_array(record_batch, "score"));

        let mut results = Vec::new();
        for i in 0..num_rows {
            let content = content_array.value(i).to_string();
            let source_type = source_type_array.value(i).to_string();
            let source_identifier = source_identifier_array.value(i).to_string();
            
            let inclusions = if let Some(array) = inclusions_array {
                Self::parse_retrieval_content_list(array, i)?
            } else {
                vec![]
            };

            let exclusions = if let Some(array) = exclusions_array {
                Self::parse_retrieval_content_list(array, i)?
            } else {
                vec![]
            };
            
            let inclusion_midpoint = if let Some(array) = midpoint_array {
                if !array.is_null(i) {
                    let midpoint_values = array.value(i);
                    let float_array = midpoint_values
                        .as_any()
                        .downcast_ref::<Float32Array>()
                        .ok_or_else(|| OxyError::RuntimeError("Midpoint values are not Float32Array".into()))?;
                    let len = float_array.len();
                    let mut midpoint = Vec::with_capacity(len);
                    for j in 0..len {
                        midpoint.push(float_array.value(j));
                    }
                    midpoint
                } else {
                    vec![]
                }
            } else {
                vec![]
            };
                        
            let inclusion_radius = if let Some(array) = radius_array {
                if !array.is_null(i) {
                    array.value(i)
                } else {
                    RETRIEVAL_DEFAULT_INCLUSION_RADIUS
                }
            } else {
                RETRIEVAL_DEFAULT_INCLUSION_RADIUS
            };
            
            let distance = if let Some(array) = distance_array {
                if !array.is_null(i) {
                    array.value(i)
                } else {
                    return Err(OxyError::RuntimeError(format!(
                        "Null distance for document '{}' - this should not be possible after vector search", 
                        source_identifier
                    )));
                }
            } else {
                return Err(OxyError::RuntimeError(
                    "Missing distance (_distance) column in search results - this should not be possible after vector search".to_string()
                ));
            };
            
            // Extract score if available (optional - is only applicable if doing full-text search)
            let score = score_array
                .filter(|array| !array.is_null(i))
                .map(|array| array.value(i));
            
            let document = Document {
                content,
                source_type,
                source_identifier,
                retrieval_inclusions: inclusions,
                retrieval_exclusions: exclusions,
                inclusion_midpoint,
                inclusion_radius,
            };
            
            let search_record = SearchRecord {
                document,
                distance,
                score,
                relevance_score: None, // Will be calculated later
            };
            
            results.push(search_record);
        }
        
        Ok(results)
    }
    
    fn parse_retrieval_content_list(list_array: &ListArray, row_index: usize) -> Result<Vec<RetrievalContent>, OxyError> {
        let mut result = Vec::new();
        
        if list_array.is_null(row_index) {
            return Ok(result);
        }
        
        let list_value = list_array.value(row_index);
        let struct_array = list_value
            .as_any()
            .downcast_ref::<StructArray>()
            .ok_or_else(|| OxyError::RuntimeError("List value is not a StructArray".into()))?;
        
        if struct_array.len() == 0 {
            return Ok(result);
        }
        
        let embedding_content_array = Self::get_struct_field_as_string_array(struct_array, "embedding_content")?;
        let embeddings_array = Self::get_struct_field_as_fixed_size_list_array(struct_array, "embeddings")?;
        
        for i in 0..struct_array.len() {
            if embedding_content_array.is_null(i) || embeddings_array.is_null(i) {
                continue;
            }
            
            let embedding_content = embedding_content_array.value(i).to_string();
            
            let embedding_values = embeddings_array.value(i);
            let float_array = embedding_values
                .as_any()
                .downcast_ref::<Float32Array>()
                .ok_or_else(|| OxyError::RuntimeError("Embedding values are not Float32Array".into()))?;
            
            let len = float_array.len();
            let mut embeddings = Vec::with_capacity(len);
            for j in 0..len {
                embeddings.push(float_array.value(j));
            }
            
            result.push(RetrievalContent {
                embedding_content,
                embeddings,
            });
        }
        
        Ok(result)
    }
} 
