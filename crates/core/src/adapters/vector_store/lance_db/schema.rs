use std::sync::Arc;
use arrow::datatypes::{DataType, Field, Fields};
use crate::config::constants::RETRIEVAL_INCLUSION_MIDPOINT_COLUMN;

pub(super) struct SchemaUtils;

impl SchemaUtils {
    pub(super) fn create_retrieval_content_struct_type(n_dims: usize) -> DataType {
        DataType::Struct(Self::create_retrieval_content_fields(n_dims))
    }

    pub(super) fn create_retrieval_content_fields(n_dims: usize) -> Fields {
        vec![
            Field::new("embedding_content", DataType::Utf8, false),
            Field::new(
                "embeddings",
                DataType::FixedSizeList(
                    Arc::new(Field::new("item", DataType::Float32, true)),
                    n_dims.try_into().unwrap(),
                ),
                false,
            ),
        ].into()
    }

    pub(super) fn create_expected_schema(n_dims: usize) -> Arc<arrow::datatypes::Schema> {
        Arc::new(arrow::datatypes::Schema::new(vec![
            Field::new("content", DataType::Utf8, false),
            Field::new("source_type", DataType::Utf8, false),
            Field::new("source_identifier", DataType::Utf8, false),
            Field::new(
                "retrieval_inclusions",
                DataType::List(Arc::new(Field::new(
                    "item",
                    Self::create_retrieval_content_struct_type(n_dims),
                    false,
                ))),
                false,
            ),
            Field::new(
                "retrieval_exclusions",
                DataType::List(Arc::new(Field::new(
                    "item",
                    Self::create_retrieval_content_struct_type(n_dims),
                    false,
                ))),
                false,
            ),
            Field::new(
                RETRIEVAL_INCLUSION_MIDPOINT_COLUMN,
                DataType::FixedSizeList(
                    Arc::new(Field::new("item", DataType::Float32, true)),
                    n_dims.try_into().unwrap(),
                ),
                false,
            ),
            Field::new("inclusion_radius", DataType::Float32, false),
        ]))
    }

    pub(super) fn schemas_match(expected: &arrow::datatypes::Schema, existing: &arrow::datatypes::Schema) -> bool {
        expected.fields().len() == existing.fields().len() &&
            expected.fields().iter().zip(existing.fields().iter()).all(|(expected, existing)| {
                expected.name() == existing.name() && expected.data_type() == existing.data_type()
            })
    }
} 
