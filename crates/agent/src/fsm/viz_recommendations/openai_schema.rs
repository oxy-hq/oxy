use async_openai::types::chat::{ChatCompletionTool, FunctionObject};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::fsm::viz_recommendations::types::ChartWithDescription;
use oxy::execute::types::VizParams;
use oxy_shared::errors::OxyError;

use super::types::{ChartDisplay, ChartRecommendation};

pub struct ChartSelectionSchema;

impl ChartSelectionSchema {
    /// Build a ChatCompletionTool where the model must select one of the provided chart configs.
    /// Uses `anyOf` with `const` values to enforce exact selection.
    ///
    /// Note: Uses `anyOf` instead of `oneOf` for OpenAI strict mode compatibility
    pub fn build_tool(
        name: &str,
        description: &str,
        recommendations: &[ChartRecommendation],
    ) -> ChatCompletionTool {
        let schemas: Vec<Value> = recommendations
            .iter()
            .map(|rec| Self::display_to_const_schema(&rec.display, &rec.rationale))
            .collect();

        let parameters = json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "definitions": {
                "ChartParamsType": {
                    "anyOf": schemas
                }
            },
            "properties": {
                "config": {
                    "$ref": "#/definitions/ChartParamsType"
                },
                "title": { "type": "string" }
            },
            "required": ["config", "title"],
            "additionalProperties": false
        });

        ChatCompletionTool {
            function: FunctionObject {
                name: name.to_string(),
                description: Some(description.to_string()),
                parameters: Some(parameters),
                strict: None,
            },
        }
    }

    /// Build a ChatCompletionTool with default name and description
    pub fn build(recommendations: &[ChartRecommendation]) -> ChatCompletionTool {
        Self::build_tool(
            "render_chart",
            "Select and render a chart visualization from the available options",
            recommendations,
        )
    }

    /// Convert a ChartDisplay into a const-based JSON schema.
    /// Uses serde serialization to avoid manual enum matching.
    fn display_to_const_schema(display: &ChartDisplay, description: &str) -> Value {
        // Serialize the display to JSON value
        let serialized = serde_json::to_value(display).unwrap_or(Value::Null);

        // Convert each field to const schema
        let mut properties = Self::value_to_const_properties(&serialized);
        // Passing back the rationale as description
        properties.insert(
            "description".to_string(),
            json!({ "type": "string", "const": description }),
        );
        let required: Vec<&str> = properties.keys().map(|s| s.as_str()).collect();

        json!({
            "type": "object",
            "description": description,
            "properties": properties,
            "required": required,
            "additionalProperties": false
        })
    }

    /// Convert a JSON value's fields into const property schemas
    fn value_to_const_properties(value: &Value) -> Map<String, Value> {
        let mut properties = Map::new();

        if let Value::Object(obj) = value {
            for (key, val) in obj {
                // Skip null values (None fields)
                if !val.is_null() {
                    properties.insert(key.clone(), json!({ "type": "string", "const": val }));
                }
            }
        }

        properties
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ChartOutput {
    config: ChartWithDescription,
    title: String,
}

impl From<ChartOutput> for VizParams {
    fn from(val: ChartOutput) -> Self {
        VizParams {
            name: val.config.description.clone(),
            title: val.title.clone(),
            config: match val.config.display {
                ChartDisplay::Line(opts) => oxy::execute::types::VizParamsType::Line(opts),
                ChartDisplay::Bar(opts) => oxy::execute::types::VizParamsType::Bar(opts),
                ChartDisplay::Pie(opts) => oxy::execute::types::VizParamsType::Pie(opts),
            },
        }
    }
}

/// Parse model output back to ChartDisplay
pub struct ChartResponseParser;

impl ChartResponseParser {
    /// Parse JSON string output from model into ChartDisplay
    pub fn parse(json_str: &str) -> Result<ChartOutput, OxyError> {
        serde_json::from_str(json_str).map_err(|e| {
            OxyError::SerializerError(format!(
                "Failed to parse chart display from model output: {}",
                e
            ))
        })
    }
}
