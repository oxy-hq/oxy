use std::fs;
use std::path::PathBuf;
use std::{fs::File, io::Write};

use super::Tool;
use crate::db::client::{STATE_DIR, get_charts_dir};
use crate::{
    config::model::{Model, OutputFormat},
    execute::agent::{ToolCall, ToolMetadata},
};
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, json};
use uuid::Uuid;

#[derive(Deserialize, Debug, JsonSchema, Serialize)]
pub struct FieldDefinition {
    #[schemars(description = "A name of the field from which to pull a data value")]
    pub field: String,

    #[schemars(description = "The aggregate function to apply to the field")]
    pub aggregate: Option<Aggregate>,

    #[schemars(description = "The type of the field")]
    #[serde(rename = "type")]
    pub field_type: Option<FieldType>,
}

impl FieldDefinition {
    pub fn to_spec(&self) -> Map<String, serde_json::Value> {
        let mut spec = Map::new();
        spec.insert("field".to_string(), json!(self.field));

        if let Some(field_type) = &self.field_type {
            spec.insert("type".to_string(), json!(field_type.as_str()));
        }
        if let Some(aggregate) = &self.aggregate {
            spec.insert("aggregate".to_string(), json!(aggregate.as_str()));
        }
        spec
    }
}

#[derive(Deserialize, Debug, JsonSchema, Serialize)]
pub enum FieldType {
    #[schemars(
        description = "Quantitative data expresses some kind of quantity. Typically this is numerical data. For example 7.3, 42.0, 12.1."
    )]
    Quantitative,

    #[schemars(
        description = "Temporal data supports date-times and times such as \"2015-03-07 12:32:17\", \"17:01\", \"2015-03-16\", \"2015\", 1552199579097 (timestamp)."
    )]
    Temporal,

    #[schemars(
        description = "Nominal data, also known as categorical data, differentiates between values based only on their names or categories. For example, gender, nationality, music genre, and name are nominal data."
    )]
    Nominal,
}

impl FieldType {
    pub fn as_str(&self) -> &'static str {
        match self {
            FieldType::Quantitative => "quantitative",
            FieldType::Temporal => "temporal",
            FieldType::Nominal => "nominal",
        }
    }
}

#[derive(Deserialize, Debug, JsonSchema, Serialize)]
pub enum Aggregate {
    #[schemars(description = "The total count of data objects in the group.")]
    Count,
    #[schemars(description = "The sum of field values.")]
    Sum,
    #[schemars(description = "The mean (average) field value. Identical to mean.")]
    Average,
    #[schemars(description = "The median field value.")]
    Median,
    #[schemars(description = "The minimum field value.")]
    Min,
    #[schemars(description = "The maximum field value.")]
    Max,
}

impl Aggregate {
    pub fn as_str(&self) -> &'static str {
        match self {
            Aggregate::Count => "count",
            Aggregate::Sum => "sum",
            Aggregate::Average => "average",
            Aggregate::Median => "median",
            Aggregate::Min => "min",
            Aggregate::Max => "max",
        }
    }
}

#[derive(Deserialize, Debug, JsonSchema, Serialize)]
pub struct VisualizeParams {
    #[schemars(description = "The chart type to use")]
    pub chart_type: ChartType,

    #[schemars(description = "X coordinates of the marks, required for bar, line chart types")]
    pub x: Option<FieldDefinition>,

    #[schemars(description = "Y coordinates of the marks, required for bar, line chart types")]
    pub y: Option<FieldDefinition>,

    #[schemars(
        description = "A field definition for the color map data fields to visual properties of the marks."
    )]
    pub color: Option<FieldDefinition>,

    #[schemars(description = "The data to use for the chart, must be a valid JSON string.")]
    pub data: String,
}

#[derive(Deserialize, Debug, JsonSchema, Serialize)]
pub enum ChartType {
    Bar,
    Line,
}

impl ChartType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ChartType::Bar => "bar",
            ChartType::Line => "line",
        }
    }
}

#[derive(Debug)]
pub struct VisualizeTool {
    pub tool_name: String,
    pub tool_description: String,
    pub output_format: OutputFormat,
    pub model: Model,
}

#[async_trait]
impl Tool for VisualizeTool {
    type Input = VisualizeParams;

    fn name(&self) -> String {
        self.tool_name.to_string()
    }

    fn description(&self) -> String {
        self.tool_description.to_string()
    }

    async fn call_internal(&self, parameters: &VisualizeParams) -> anyhow::Result<ToolCall> {
        serde_json::from_str::<serde_json::Value>(&parameters.data)
            .map_err(|e| anyhow::anyhow!("Invalid JSON data: {}", e))?;

        let tmp_chart_dir = get_charts_dir();

        let file_path = tmp_chart_dir.join(format!("{}.json", Uuid::new_v4()));

        let mut encoding = Map::new();

        if let Some(x) = &parameters.x {
            encoding.insert("x".to_string(), json!(x.to_spec()));
        }
        if let Some(y) = &parameters.y {
            encoding.insert("y".to_string(), json!(y.to_spec()));
        }
        if let Some(color) = &parameters.color {
            encoding.insert("color".to_string(), json!(color.to_spec()));
        }

        let spec = json!({
            "$schema": "https://vega.github.io/schema/vega-lite/v5.json",
            "data": {
                "values": parameters.data
            },
            "mark": parameters.chart_type.as_str(),
            "encoding": encoding
        });

        let mut file = File::create(&file_path)?;
        file.write_all(spec.to_string().as_bytes())?;

        Ok(ToolCall {
            name: self.name(),
            output: format!(
                "Use this markdown directive to render the chart :chart{{file_path={}}} directly in the final answer.",
                file_path.display()
            ),
            metadata: Some(ToolMetadata::Visualize {
                file_path: file_path.display().to_string(),
            }),
        })
    }
}
