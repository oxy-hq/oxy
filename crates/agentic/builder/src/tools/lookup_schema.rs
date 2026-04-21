use agentic_core::tools::{ToolDef, ToolError};
use serde_json::{Value, json};

use crate::schema_provider::BuilderSchemaProvider;

pub fn lookup_schema_def(provider: &dyn BuilderSchemaProvider) -> ToolDef {
    let type_list = provider.supported_types().join(", ");
    ToolDef {
        name: "lookup_schema",
        description: Box::leak(format!(
            "Look up the JSON schema for a named Oxy object type. Returns the full JSON Schema describing all fields, types, and constraints. \
             Supported types: {type_list}."
        ).into_boxed_str()),
        parameters: json!({
            "type": "object",
            "properties": {
                "object_name": {
                    "type": "string",
                    "description": "Name of the Oxy object type to look up (e.g. 'AgentConfig', 'Workflow', 'Task', 'View', 'Dimension')"
                }
            },
            "required": ["object_name"],
            "additionalProperties": false
        }),
        ..Default::default()
    }
}

pub fn execute_lookup_schema(
    params: &Value,
    provider: &dyn BuilderSchemaProvider,
) -> Result<Value, ToolError> {
    let object_name = params["object_name"]
        .as_str()
        .ok_or_else(|| ToolError::BadParams("missing 'object_name'".into()))?;

    let schema = provider.get_schema(object_name).ok_or_else(|| {
        let supported = provider.supported_types().join(", ");
        ToolError::BadParams(format!(
            "unknown object type '{object_name}'. Supported types: {supported}"
        ))
    })?;

    Ok(json!({ "object_name": object_name, "schema": schema }))
}
