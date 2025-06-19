use std::{fs::File, io::Write};

use crate::{
    db::client::get_charts_dir,
    errors::OxyError,
    execute::{
        Executable, ExecutionContext,
        types::{Chunk, Output, Prompt},
    },
    tools::types::VisualizeInput,
};

use serde_json::json;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct VisualizeExecutable;
impl VisualizeExecutable {
    pub fn new() -> Self {
        Self
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

        let tmp_chart_dir = get_charts_dir();
        let file_path = tmp_chart_dir.join(format!("{}.json", Uuid::new_v4()));

        let mut file = File::create(&file_path).map_err(|e| anyhow::anyhow!(e))?;
        file.write_all(chart_config.to_string().as_bytes())
            .map_err(|e| anyhow::anyhow!(e))?;

        Ok(Output::Text(format!(
            "Use this markdown directive to render the chart \":chart{{chart_src={}}}\" directly in the final answer.",
            file_path.display()
        )))
    }
}
