use async_openai::types::{ChatCompletionTool, ChatCompletionToolType, FunctionObject};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Visualize {
    #[serde(default = "default_viz_name")]
    pub name: String,
    #[serde(default = "default_viz_description")]
    pub description: String,
    #[serde(default = "default_viz_instruction")]
    pub instruction: String,
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    pub model: Option<String>,
}

fn default_viz_name() -> String {
    "visualize".to_string()
}

fn default_viz_description() -> String {
    "Generates visualizations based on the provided data and instructions.".to_string()
}

fn default_viz_instruction() -> String {
    "Create a visualization that effectively represents the data and insights. Choose the appropriate chart type and ensure clarity and accuracy.".to_string()
}

fn default_max_retries() -> u32 {
    5
}

impl Visualize {
    pub fn get_tool(&self) -> ChatCompletionTool {
        let schema = serde_json::from_str(VIZ_SCHEMA).expect("Failed to parse VizParams schema");
        ChatCompletionTool {
            r#type: ChatCompletionToolType::Function,
            function: FunctionObject {
                name: self.name.clone(),
                description: Some(self.description.clone()),
                parameters: Some(schema),
                strict: Some(true),
            },
        }
    }
}

// oneOf is not supported by strict mode in OpenAI
// so we need to define the schema manually to replace oneOf with anyOf
// and include all fields in the required array
const VIZ_SCHEMA: &str = r##"
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "additionalProperties": false,
  "definitions": {
    "VizParamsType": {
      "anyOf": [
        {
          "additionalProperties": false,
          "properties": {
            "data": {
              "description": "reference data output from a table using table name",
              "type": "string"
            },
            "series": { "type": ["string", "null"] },
            "title": { "type": ["string", "null"] },
            "type": { "enum": ["line"], "type": "string" },
            "x": { "type": "string" },
            "x_axis_label": { "type": ["string", "null"] },
            "y": { "type": "string" },
            "y_axis_label": { "type": ["string", "null"] }
          },
          "required": ["data", "series", "title", "type", "x", "x_axis_label", "y", "y_axis_label"],
          "type": "object"
        },
        {
          "additionalProperties": false,
          "properties": {
            "data": {
              "description": "reference data output from a table using table name",
              "type": "string"
            },
            "series": { "type": ["string", "null"] },
            "title": { "type": ["string", "null"] },
            "type": { "enum": ["bar"], "type": "string" },
            "x": { "type": "string" },
            "y": { "type": "string" }
          },
          "required": ["data", "series", "title", "type", "x", "y"],
          "type": "object"
        },
        {
          "additionalProperties": false,
          "properties": {
            "data": {
              "description": "reference data output from a table using table name",
              "type": "string"
            },
            "name": { "type": "string" },
            "title": { "type": ["string", "null"] },
            "type": { "enum": ["pie"], "type": "string" },
            "value": { "type": "string" }
          },
          "required": ["data", "name", "title", "type", "value"],
          "type": "object"
        }
      ]
    }
  },
  "properties": {
    "config": { "$ref": "#/definitions/VizParamsType" },
    "name": { "type": "string" },
    "title": { "type": "string" }
  },
  "required": ["config", "name", "title"],
  "title": "VizParams",
  "type": "object"
}
"##;
