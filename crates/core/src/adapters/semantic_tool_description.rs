use crate::config;
use oxy_semantic::{self, SemanticLayer, Topic, View, parse_semantic_layer_from_dir};
use oxy_shared::errors::OxyError;

/// Get enhanced description for semantic query tool with semantic layer metadata
pub fn get_semantic_query_description(
    semantic_tool: &crate::config::model::SemanticQueryTool,
    config_manager: &config::ConfigManager,
) -> Result<String, OxyError> {
    let semantic_layer = load_semantic_layer(config_manager)?;

    let mut description = String::new();
    description.push_str(&semantic_tool.description);
    description.push_str("\n\n**Semantic layer:**\n");

    get_topics_metadata(
        &mut description,
        &semantic_layer,
        semantic_tool.topic.as_deref(),
    )?;

    tracing::info!("Semantic layer description: {}", description);
    Ok(description)
}

/// Build semantic layer description for a specific topic
/// Used by MCP tools and other contexts where we have a Topic directly
pub fn build_semantic_topic_description(topic: &Topic, semantic_layer: &SemanticLayer) -> String {
    let mut description = String::new();
    description.push_str(&topic.description);
    description.push_str("\n\n**Semantic layer:**\n");
    build_topic_metadata(&mut description, topic, semantic_layer);
    description
}

fn load_semantic_layer(config_manager: &config::ConfigManager) -> Result<SemanticLayer, OxyError> {
    let semantic_dir = config_manager.semantics_path();

    if !semantic_dir.exists() {
        return Err(OxyError::ConfigurationError(
            "No semantic layer metadata found. Please ensure you have semantic layer definitions in the 'semantics' directory.".to_string()
        ));
    }

    let parse_result =
        parse_semantic_layer_from_dir(&semantic_dir, config_manager.get_globals_registry())
            .map_err(|e| {
                OxyError::ConfigurationError(format!("Failed to parse semantic layer: {}", e))
            })?;

    Ok(parse_result.semantic_layer)
}

fn get_topics_metadata(
    description: &mut String,
    semantic_layer: &SemanticLayer,
    specified_topic: Option<&str>,
) -> Result<(), OxyError> {
    let Some(topics) = &semantic_layer.topics else {
        return Err(OxyError::ConfigurationError(
            "No topics found in the semantic layer.".to_string(),
        ));
    };

    if topics.is_empty() {
        return Err(OxyError::ConfigurationError(
            "No topics available in the semantic layer.".to_string(),
        ));
    }

    let filtered_topics = filter_topics(topics, specified_topic);

    if filtered_topics.is_empty() {
        return Err(build_no_topics_error(specified_topic));
    }

    for topic in filtered_topics {
        build_topic_metadata(description, topic, semantic_layer);
    }

    Ok(())
}

fn filter_topics<'a>(topics: &'a [Topic], specified_topic: Option<&str>) -> Vec<&'a Topic> {
    match specified_topic {
        Some(topic_name) => topics
            .iter()
            .filter(|topic| topic.name == topic_name)
            .collect(),
        None => topics.iter().collect(),
    }
}

fn build_no_topics_error(specified_topic: Option<&str>) -> OxyError {
    match specified_topic {
        Some(topic_name) => OxyError::ConfigurationError(format!(
            "Specified topic '{}' not found in the semantic layer.",
            topic_name
        )),
        None => {
            OxyError::ConfigurationError("No topics available in the semantic layer.".to_string())
        }
    }
}

fn build_topic_metadata(description: &mut String, topic: &Topic, semantic_layer: &SemanticLayer) {
    description.push_str(&format!("\n# Topic: {}\n", topic.name));
    if let Some(base_view) = &topic.base_view {
        description.push_str(&format!("\nBase view: {}\n", base_view));
    }
    description.push_str(&format!("{}\n", topic.description));

    let topic_views = get_topic_views(topic, semantic_layer);

    for view in &topic_views {
        build_view_metadata(description, view);
    }
}

fn get_topic_views<'a>(topic: &Topic, semantic_layer: &'a SemanticLayer) -> Vec<&'a View> {
    semantic_layer
        .views
        .iter()
        .filter(|view| topic.views.contains(&view.name))
        .collect()
}

fn build_view_metadata(description: &mut String, view: &View) {
    description.push_str(&format!("\n## View: {}\n", view.name));

    build_measures_metadata(description, view);
    build_dimensions_metadata(description, view);
}

fn build_measures_metadata(description: &mut String, view: &View) {
    let Some(measures) = &view.measures else {
        return;
    };

    if measures.is_empty() {
        return;
    }

    description.push_str("### Measures:\n");
    for measure in measures {
        let display_info = if let Some(ref desc) = measure.description {
            if desc.is_empty() {
                measure.measure_type.to_string()
            } else {
                format!("{}: {}", measure.measure_type, desc)
            }
        } else {
            measure.measure_type.to_string()
        };

        let mut measure_line = format!("- {}: {}", measure.name, display_info);

        // Add sample values if available
        if let Some(samples) = &measure.samples
            && !samples.is_empty()
        {
            let sample_text = if samples.len() == 1 {
                samples[0].clone()
            } else {
                samples.join(", ")
            };
            measure_line.push_str(&format!(" (samples: {})", sample_text));
        }

        // Add synonyms if available
        if let Some(synonyms) = &measure.synonyms
            && !synonyms.is_empty()
        {
            measure_line.push_str(&format!(" [synonyms: {}]", synonyms.join(", ")));
        }

        measure_line.push('\n');
        description.push_str(&measure_line);
    }
}

fn build_dimensions_metadata(description: &mut String, view: &View) {
    if view.dimensions.is_empty() {
        return;
    }

    description.push_str("### Dimensions:\n");
    for dimension in &view.dimensions {
        let display_info = if let Some(ref desc) = dimension.description {
            if desc.is_empty() {
                dimension.dimension_type.to_string()
            } else {
                format!("{}: {}", dimension.dimension_type, desc)
            }
        } else {
            dimension.dimension_type.to_string()
        };

        let mut dimension_line = format!("- {}: {}", dimension.name, display_info);

        // Add sample values if available
        if let Some(samples) = &dimension.samples
            && !samples.is_empty()
        {
            let sample_text = if samples.len() == 1 {
                samples[0].clone()
            } else {
                samples.join(", ")
            };
            dimension_line.push_str(&format!(" (samples: {})", sample_text));
        }

        // Add synonyms if available
        if let Some(synonyms) = &dimension.synonyms
            && !synonyms.is_empty()
        {
            dimension_line.push_str(&format!(" [synonyms: {}]", synonyms.join(", ")));
        }

        dimension_line.push('\n');
        description.push_str(&dimension_line);
    }
}
