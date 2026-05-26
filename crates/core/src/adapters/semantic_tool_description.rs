use oxy_semantic::{self, SemanticLayer, Topic, View};

/// Build semantic layer description for a specific topic.
/// Used by MCP tools and other contexts where we have a Topic directly.
///
/// Output order:
///   {topic.description}\n\n**Semantic layer:**\n{topic name/views/measures/dimensions}
///
/// The description leads so it reads naturally as the MCP tool's primary description.
/// `build_topic_metadata` (used for multi-topic listings) keeps the description inside
/// the `# Topic:` block for a different, list-oriented layout.
pub fn build_semantic_topic_description(topic: &Topic, semantic_layer: &SemanticLayer) -> String {
    let mut out = String::new();
    if let Some(ref desc) = topic.description {
        out.push_str(desc);
        out.push_str("\n\n");
    }
    out.push_str("**Semantic layer:**\n");
    // Push topic header + views without re-including description (already at the top).
    out.push_str(&format!("\n# Topic: {}\n", topic.name));
    if let Some(base_view) = &topic.base_view {
        out.push_str(&format!("\nBase view: {}\n", base_view));
    }
    build_topic_views(&mut out, topic, semantic_layer);
    out
}

/// Append the view/measure/dimension blocks for a topic, without the topic header or description.
fn build_topic_views(description: &mut String, topic: &Topic, semantic_layer: &SemanticLayer) {
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
