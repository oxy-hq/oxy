use std::{fs::File, io::Write};

use crate::{
    errors::OxyError,
    execute::{
        Executable, ExecutionContext,
        types::{Chunk, Output, Prompt},
    },
    tools::{tool::Tool, types::VisualizeInput},
};

use serde_json::{Map, json};
use uuid::Uuid;

use super::types::VisualizeParams;

#[derive(Debug, Clone)]
pub struct VisualizeExecutable;

impl VisualizeExecutable {
    pub fn new() -> Self {
        Self
    }
}

impl Tool for VisualizeExecutable {
    type Param = VisualizeParams;
    type Output = String;

    fn serialize_output(&self, output: &Self::Output) -> Result<String, OxyError> {
        Ok(output.to_string())
    }
}

#[async_trait::async_trait]
impl Executable<VisualizeInput> for VisualizeExecutable {
    type Response = Output;

    async fn execute(
        &mut self,
        execution_context: &ExecutionContext,
        input: VisualizeInput,
    ) -> Result<Self::Response, OxyError> {
        let VisualizeInput { param } = input;
        execution_context
            .write_chunk(Chunk {
                key: None,
                delta: Output::Prompt(Prompt::new("Visualizing data...".to_string())).into(),
                finished: true,
            })
            .await?;
        serde_json::from_str::<serde_json::Value>(&param.data)
            .map_err(|e| anyhow::anyhow!("Invalid JSON data: {}", e))?;

        let file_path = format!("/tmp/{}.json", Uuid::new_v4());

        let mut encoding = Map::new();

        if let Some(x) = &param.x {
            encoding.insert("x".to_string(), json!(x.to_spec()));
        }
        if let Some(y) = &param.y {
            encoding.insert("y".to_string(), json!(y.to_spec()));
        }
        if let Some(color) = &param.color {
            encoding.insert("color".to_string(), json!(color.to_spec()));
        }

        let spec = json!({
            "$schema": "https://vega.github.io/schema/vega-lite/v5.json",
            "data": {
                "values": param.data
            },
            "mark": param.chart_type.as_str(),
            "encoding": encoding
        });

        let mut file = File::create(&file_path).map_err(|e| anyhow::anyhow!(e))?;
        file.write_all(spec.to_string().as_bytes())
            .map_err(|e| anyhow::anyhow!(e))?;

        Ok(Output::Text(format!(
            "Use this markdown directive to render the chart \":chart{{file_path={}}}\" directly in the final answer.",
            file_path
        )))
    }
}
