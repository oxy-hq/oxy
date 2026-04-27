use crate::config::constants::RETRIEVAL_EMBEDDINGS_COLUMN;
use arrow57::datatypes::{DataType, Field};
use std::sync::Arc;

pub(super) struct SchemaUtils;

impl SchemaUtils {
    pub(super) fn create_retrieval_schema(n_dims: usize) -> Arc<arrow57::datatypes::Schema> {
        Arc::new(arrow57::datatypes::Schema::new(vec![
            Field::new("content", DataType::Utf8, false),
            Field::new("source_type", DataType::Utf8, false),
            Field::new("source_identifier", DataType::Utf8, false),
            Field::new("upsert_key", DataType::Utf8, false),
            Field::new("embedding_content", DataType::Utf8, false),
            Field::new(
                RETRIEVAL_EMBEDDINGS_COLUMN,
                DataType::FixedSizeList(
                    Arc::new(Field::new("item", DataType::Float32, true)),
                    n_dims.try_into().unwrap(),
                ),
                false,
            ),
            Field::new("radius", DataType::Float32, false),
        ]))
    }

    pub(super) fn schemas_match(
        expected: &arrow57::datatypes::Schema,
        existing: &arrow57::datatypes::Schema,
    ) -> bool {
        expected.fields().len() == existing.fields().len()
            && expected
                .fields()
                .iter()
                .zip(existing.fields().iter())
                .all(|(expected, existing)| {
                    expected.name() == existing.name()
                        && expected.data_type() == existing.data_type()
                })
    }
}
