use crate::{config::ConfigManager, config::model::LookerQueryTool};
use oxy_shared::errors::OxyError;

/// Get enhanced description for Looker query tool with explore metadata
pub async fn get_looker_query_description(
    looker_tool: &LookerQueryTool,
    config: &ConfigManager,
) -> Result<String, OxyError> {
    let state_dir = config.resolve_state_dir().await?;

    let storage = oxy_looker::MetadataStorage::new(
        state_dir.join(".looker"),
        config.project_path().join("looker"),
        looker_tool.integration.clone(),
    );

    let metadata = storage
        .load_merged_metadata(&looker_tool.model, &looker_tool.explore)
        .map_err(|e| {
            OxyError::ConfigurationError(format!(
                "Failed to load Looker metadata for {}.{}: {}. Run 'oxy looker sync' to synchronize metadata.",
                looker_tool.model, looker_tool.explore, e
            ))
        })?;

    let mut description = String::new();
    description.push_str(&looker_tool.description);
    description.push_str(&format!(
        "\n\n**Explore: {}.{}**\n",
        metadata.model, metadata.name
    ));

    if let Some(label) = &metadata.label {
        description.push_str(&format!("Label: {}\n", label));
    }
    if let Some(desc) = &metadata.description {
        description.push_str(&format!("{}\n", desc));
    }

    description.push_str("\n**Available Views and Fields:**\n");

    for view in &metadata.views {
        description.push_str(&format!("\n*View: {}*\n", view.name));

        if !view.dimensions.is_empty() {
            description.push_str("Dimensions:\n");
            for dim in &view.dimensions {
                description.push_str(&format!("- {}", dim.name));
                if let Some(label) = &dim.label {
                    description.push_str(&format!(" ({})", label));
                }
                if let Some(data_type) = &dim.data_type {
                    description.push_str(&format!(" [{}]", data_type));
                }
                if let Some(desc) = &dim.description {
                    description.push_str(&format!(" - {}", desc));
                }
                if let Some(hint) = &dim.agent_hint {
                    description.push_str(&format!(" [Hint: {}]", hint));
                }
                description.push('\n');
            }
        }

        if !view.measures.is_empty() {
            description.push_str("Measures:\n");
            for measure in &view.measures {
                description.push_str(&format!("- {}", measure.name));
                if let Some(label) = &measure.label {
                    description.push_str(&format!(" ({})", label));
                }
                if let Some(data_type) = &measure.data_type {
                    description.push_str(&format!(" [{}]", data_type));
                }
                if let Some(desc) = &measure.description {
                    description.push_str(&format!(" - {}", desc));
                }
                if let Some(hint) = &measure.agent_hint {
                    description.push_str(&format!(" [Hint: {}]", hint));
                }
                description.push('\n');
            }
        }
    }

    description.push_str("\n**Usage Notes:**\n");
    description.push_str("- Field names must use format: view_name.field_name\n");
    description.push_str("- Filters map field names to Looker filter expressions. IMPORTANT: Use Looker's native filter syntax, NOT SQL expressions.\n");
    description.push_str("  Valid Looker date filter examples:\n");
    description.push_str("    - \"last 30 days\"\n");
    description.push_str("    - \"1 month ago\"\n");
    description.push_str("    - \"last 3 months\"\n");
    description.push_str("    - \"this month\"\n");
    description.push_str("    - \"yesterday\"\n");
    description.push_str("    - \"2024-01-01 to 2024-12-31\"\n");
    description.push_str("    - \"before 2024-01-01\"\n");
    description.push_str("    - \"after 2024-06-01\"\n");
    description.push_str("  NEVER use SQL expressions like 'current_date - interval ...' or 'DATE_SUB(...)' — they will cause errors.\n");
    description.push_str("  Valid Looker string/number filter examples:\n");
    description.push_str("    - \"value\" (equals)\n");
    description.push_str("    - \"%value%\" (contains)\n");
    description.push_str("    - \">=100\" (greater than or equal)\n");
    description.push_str("    - \"[10,20]\" (between)\n");
    description.push_str("- Set appropriate limits for large datasets\n");
    description.push_str(
        "- Sort by relevant fields using field names, prefix with \"-\" for descending\n",
    );

    tracing::info!("Looker query description: {}", description);
    Ok(description)
}
