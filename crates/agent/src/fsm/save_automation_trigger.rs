use std::path::PathBuf;

use slugify::slugify;

use crate::fsm::{
    save_automation_config::SaveAutomation,
    state::MachineContext,
    types::{Artifact, TableSource},
};
use oxy::config::model::{
    ExecuteSQLTask, RouteRetrievalConfig, SQL, SemanticQueryTask, Task, TaskType, Workflow,
};
use oxy::constants::{AUTOMATION_FILE_EXTENSION, AUTOMATION_SAVED_DIR};
use oxy::execute::{ExecutionContext, builders::fsm::Trigger};
use oxy_shared::errors::OxyError;

pub struct AutoSaveAutomation<S> {
    config: SaveAutomation,
    objective: String,
    _state: std::marker::PhantomData<S>,
}

impl<S> AutoSaveAutomation<S> {
    pub fn new(config: SaveAutomation, objective: String) -> Self {
        Self {
            config,
            objective,
            _state: std::marker::PhantomData,
        }
    }
}

/// Convert collected artifacts from the MachineContext into Workflow Tasks.
///
/// Only Table artifacts (sourced from SQL or Semantic queries) are converted.
/// Viz, Insight, and DataApp artifacts are skipped — they cannot be replayed
/// as deterministic workflow steps with the information available here.
fn artifacts_to_tasks(artifacts: &[Artifact]) -> Vec<Task> {
    artifacts
        .iter()
        .enumerate()
        .filter_map(|(i, artifact)| match artifact {
            Artifact::Table {
                source: TableSource::SQL { sql, database },
                ..
            } => Some(Task {
                name: format!("query_{}", i + 1),
                task_type: TaskType::ExecuteSQL(ExecuteSQLTask {
                    sql: SQL::Query {
                        sql_query: sql.clone(),
                    },
                    database: database.clone(),
                    export: None,
                    dry_run_limit: None,
                    variables: None,
                }),
                cache: None,
            }),
            Artifact::Table {
                source: TableSource::Semantic { task },
                ..
            } => Some(Task {
                name: format!("semantic_query_{}", i + 1),
                task_type: TaskType::SemanticQuery(SemanticQueryTask {
                    query: task.query.clone(),
                    export: task.export.clone(),
                    variables: task.variables.clone(),
                }),
                cache: None,
            }),
            // Viz, Insight, and DataApp artifacts cannot be represented as
            // executable workflow tasks with currently stored information.
            Artifact::Viz { .. } | Artifact::Insight { .. } | Artifact::DataApp { .. } => None,
        })
        .collect()
}

#[async_trait::async_trait]
impl Trigger for AutoSaveAutomation<MachineContext> {
    type State = MachineContext;

    async fn run(
        &self,
        execution_context: &ExecutionContext,
        state: &mut Self::State,
    ) -> Result<(), OxyError> {
        tracing::info!(
            "Running SaveAutomation trigger for objective: {}",
            self.objective
        );

        let artifacts = state.list_artifacts();
        let tasks = artifacts_to_tasks(artifacts);

        if tasks.is_empty() {
            tracing::warn!("No artifacts to save as automation");
            state.add_message(
                "No steps were captured to save as an automation. Execute some queries or analyses first."
                    .to_string(),
            );
            return Ok(());
        }

        let automation_name = {
            let slug = slugify!(&self.objective, separator = "_");
            if slug.is_empty() {
                format!("automation_{}", uuid::Uuid::new_v4().simple())
            } else {
                slug
            }
        };

        let retrieval = self.config.retrieval.clone().or_else(|| {
            Some(RouteRetrievalConfig {
                include: vec![self.objective.clone()],
                exclude: vec![],
            })
        });

        let workflow = Workflow {
            name: automation_name.clone(),
            description: self.objective.clone(),
            tasks,
            tests: vec![],
            variables: None,
            retrieval,
            consistency_prompt: None,
        };

        // Write automation to file
        let automation_dir = execution_context
            .project
            .config_manager
            .resolve_file(AUTOMATION_SAVED_DIR)
            .await?;
        let automation_dir = PathBuf::from(automation_dir);
        tokio::fs::create_dir_all(&automation_dir)
            .await
            .map_err(|e| {
                OxyError::RuntimeError(format!("Failed to create automation directory: {}", e))
            })?;
        let yaml = serde_yaml::to_string(&workflow).map_err(|e| {
            OxyError::RuntimeError(format!("Failed to serialize automation: {}", e))
        })?;

        // Use create_new(true) to atomically find a unique path and write the file,
        // avoiding both blocking exists() calls and the TOCTOU race.
        let mut candidate_name = automation_name.clone();
        let mut counter = 2u32;
        let automation_path = loop {
            let path =
                automation_dir.join(format!("{}{}", candidate_name, AUTOMATION_FILE_EXTENSION));
            match tokio::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
                .await
            {
                Ok(mut file) => {
                    use tokio::io::AsyncWriteExt;
                    file.write_all(yaml.as_bytes()).await.map_err(|e| {
                        OxyError::RuntimeError(format!("Failed to write automation file: {}", e))
                    })?;
                    break path;
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    candidate_name = format!("{}_{}", automation_name, counter);
                    counter += 1;
                }
                Err(e) => {
                    return Err(OxyError::RuntimeError(format!(
                        "Failed to create automation file: {}",
                        e
                    )));
                }
            }
        };

        tracing::info!("Saved automation to: {}", automation_path.display());
        state.add_message(format!(
            "Automation '{}' saved successfully to {}",
            automation_name,
            automation_path.display()
        ));

        Ok(())
    }
}
