use std::{fs::File, io::Write};

use crate::execute::{
    Executable, ExecutionContext,
    types::{Chunk, Output, Prompt},
};
use oxy_shared::errors::OxyError;

use serde_json::json;
use uuid::Uuid;

use super::types::VisualizeParams;

#[derive(Debug, Clone)]
pub struct VisualizeExecutable;

impl Default for VisualizeExecutable {
    fn default() -> Self {
        Self::new()
    }
}

impl VisualizeExecutable {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl Executable<VisualizeParams> for VisualizeExecutable {
    type Response = Output;

    async fn execute(
        &mut self,
        execution_context: &ExecutionContext,
        param: VisualizeParams,
    ) -> Result<Self::Response, OxyError> {
        execution_context
            .write_chunk(Chunk {
                key: None,
                delta: Output::Prompt(Prompt::new("Visualizing data...".to_string())),
                finished: true,
            })
            .await?;

        let chart_config = json!({
            "xAxis": param.x_axis,
            "yAxis": param.y_axis,
            "series": param.series,
            "title": param.title,
        });

        let tmp_chart_dir = execution_context
            .project
            .config_manager
            .get_charts_dir()
            .await?;
        let file_name = format!("{}.json", Uuid::new_v4());
        let file_path = tmp_chart_dir.join(&file_name);

        let mut file =
            File::create(&file_path).map_err(|e| OxyError::RuntimeError(e.to_string()))?;
        file.write_all(chart_config.to_string().as_bytes())
            .map_err(|e| OxyError::RuntimeError(e.to_string()))?;

        Ok(Output::Text(format!(
            "Use this markdown directive to render the chart \":chart{{chart_src={}}}\" directly in the final answer.",
            file_name
        )))
    }
}
