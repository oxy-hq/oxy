use std::collections::HashMap;

use crate::{
    adapters::vector_store::types::RetrievalObject, errors::OxyError,
    service::retrieval::EnumIndexManager,
};

/// Build retrieval objects from parameterized templates based on a query.
/// Group by source and return one RetrievalObject per source_identifier.
pub async fn build_parameterized_retrieval_objects(
    enum_index_manager: &EnumIndexManager,
    query: &str,
) -> Result<Vec<RetrievalObject>, OxyError> {
    let rendered_templates = match enum_index_manager.render_items_for_query(query).await {
        Ok(items) => items,
        Err(err) => {
            tracing::error!("Failed to render enum-based retrieval items: {}", err);
            return Ok(Vec::new());
        }
    };

    if rendered_templates.is_empty() {
        tracing::debug!("No enum-based rendered templates for query, skipping parameterized build");
        return Ok(Vec::new());
    }

    let mut retrieval_content_by_source: HashMap<String, (String, Vec<String>, Vec<String>)> =
        HashMap::new();
    for item in rendered_templates.into_iter() {
        let source_identifier = item.source_identifier.clone();
        let (_source_type, inclusions, exclusions) = retrieval_content_by_source
            .entry(source_identifier)
            .or_insert_with(|| (item.source_type.clone(), Vec::new(), Vec::new()));
        if item.is_exclusion {
            exclusions.push(item.rendered_text);
        } else {
            inclusions.push(item.rendered_text);
        }
    }

    let mut results: Vec<RetrievalObject> = Vec::with_capacity(retrieval_content_by_source.len());
    for (source_identifier, (source_type, inclusions, exclusions)) in
        retrieval_content_by_source.into_iter()
    {
        results.push(RetrievalObject {
            source_identifier,
            source_type,
            inclusions,
            exclusions,
            is_child: true,
            ..Default::default()
        });
    }

    Ok(results)
}
