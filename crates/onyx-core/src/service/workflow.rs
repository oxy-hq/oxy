use serde::Serialize;
use std::path::PathBuf;

use crate::{
    config::{model::Workflow, ConfigBuilder},
    errors::OnyxError,
    utils::find_project_path,
};

#[derive(Serialize)]
pub struct WorkflowInfo {
    pub name: String,
    pub path: String,
}

pub async fn list_workflows() -> Result<Vec<WorkflowInfo>, OnyxError> {
    let project_path = find_project_path()?;
    let config = ConfigBuilder::new()
        .with_project_path(project_path.clone())?
        .build()
        .await?;

    let workflow_paths = config.list_workflows().await?;
    let mut workflows = Vec::new();

    for path in workflow_paths {
        if let Some(name) = path
            .file_stem()
            .and_then(|s| s.to_str())
            .and_then(|s| s.strip_suffix(".workflow"))
        {
            workflows.push(WorkflowInfo {
                name: name.to_string(),
                path: path
                    .strip_prefix(project_path.clone())
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .to_string(),
            });
        }
    }

    Ok(workflows)
}

pub async fn get_workflow(relative_path: PathBuf) -> Result<Workflow, OnyxError> {
    let project_path = find_project_path()?;
    let config = ConfigBuilder::new()
        .with_project_path(project_path.clone())?
        .build()
        .await?;

    let full_workflow_path = project_path.join(&relative_path);
    let workflow = config.resolve_workflow(&full_workflow_path).await?;

    Ok(workflow)
}
