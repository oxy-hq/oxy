use async_openai::types::chat::{ChatCompletionTool, FunctionObject};
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
    "Generates visualizations based on the provided data and instructions.
    Supported chart types include line charts, bar charts, and pie charts."
        .to_string()
}

fn default_viz_instruction() -> String {
    "You are an expert data visualization assistant. Select the most appropriate chart type based on the data structure and objective.

## Supported Chart Types

**Line Chart** - For trends over time or continuous data
- Requires: x-axis (time/continuous), y-axis (numeric values), optional series for multiple lines
- Best for: Time series, progress tracking, comparing trends
- Example: Monthly revenue over 12 months, daily active users, stock prices

**Bar Chart** - For comparing categories
- Requires: x-axis (categories), y-axis (numeric values), optional series for grouped bars
- Best for: Comparing quantities across categories, rankings, distributions
- Example: Sales by region, products by revenue, employee counts by department

**Pie Chart** - For showing proportions of a whole
- Requires: name (category), value (numeric representing share)
- Best for: Market share, percentage breakdowns, composition (use only with 2-10 categories)
- Example: Budget allocation by category, browser usage share

## Selection Guidelines

1. Examine available data columns and their types
2. Match data structure to chart requirements:
   - Time/date column + numeric → Line chart
   - Categories + numeric → Bar chart
   - Categories with percentages/parts of whole → Pie chart
3. Consider the objective - what insight should the visualization reveal?
4. Verify data quality: no nulls in required columns, adequate row count (>1)

## Error Handling

Only return an error if:
- Data lacks required columns (e.g., no numeric column for y-axis)
- Data is empty or has only 1 row (insufficient for visualization)
- Requested chart type doesn't match data structure

Error format: Clearly state what's missing and which chart types could work with available data.".to_string()
}

fn default_max_retries() -> u32 {
    5
}

impl Visualize {
    pub fn get_tool(&self) -> ChatCompletionTool {
        let schema = serde_json::from_str(VIZ_SCHEMA).expect("Failed to parse VizParams schema");
        ChatCompletionTool {
            function: FunctionObject {
                name: self.name.clone(),
                description: Some(self.description.clone()),
                parameters: Some(schema),
                strict: None,
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
            "name": { "type": "string", "description": "The name of the category column to group_by in the data." },
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
