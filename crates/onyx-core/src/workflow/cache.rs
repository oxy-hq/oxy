use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

use minijinja::Value;

use crate::execute::core::ExecutionContext;
use crate::execute::exporter::get_file_directories;
use crate::execute::workflow::WorkflowEvent;
use crate::StyledText;
use crate::{ai::agent::AgentResult, config::model::StepCache, errors::OnyxError};

async fn render_cache_path(
    execution_context: &mut ExecutionContext<'_, WorkflowEvent>,
    cache_path: &str,
) -> Result<String, OnyxError> {
    execution_context
        .renderer
        .render_async(
            cache_path,
            Value::from_serialize(&execution_context.get_context()),
        )
        .await
}

pub async fn get_agent_cache(
    project_path: &PathBuf,
    step_cache: Option<StepCache>,
    execution_context: &mut ExecutionContext<'_, WorkflowEvent>,
) -> Result<Option<AgentResult>, OnyxError> {
    let Some(cache) = step_cache else {
        return Ok(None);
    };

    if cache.enabled {
        let cache_file_path = render_cache_path(execution_context, &cache.path).await?;
        let cache_output = std::fs::read_to_string(project_path.join(cache_file_path)).ok();
        if let Some(json) = cache_output {
            let cache_result_output = serde_json::from_str::<AgentResult>(&json).map_err(|e| {
                OnyxError::RuntimeError(format!("Error in parsing cache file: {}", e))
            })?;
            return Ok(Some(cache_result_output));
        }
    }
    Ok(None)
}

pub fn write_agent_cache(path: &PathBuf, result: &AgentResult) {
    match get_file_directories(path) {
        Ok(file_path) => {
            let mut file = match File::create(&file_path) {
                Ok(f) => f,
                Err(e) => {
                    println!(
                        "{}",
                        format!(
                            "Error creating directories for path '{}': {}",
                            path.display(),
                            e
                        )
                        .warning()
                    );

                    return;
                }
            };
            let _ = file
                .write_all(serde_json::to_string(&result).unwrap().as_bytes())
                .map_err(|e| {
                    println!(
                        "{}",
                        format!("Error writing to cache file: {}", e).warning()
                    );
                });
        }
        Err(e) => println!(
            "{}",
            format!(
                "Error creating directories for path '{}': {}",
                path.display(),
                e
            )
            .warning()
        ),
    }
}
