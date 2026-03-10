use std::path::PathBuf;

use slugify::slugify;

use crate::fsm::{
    save_automation_config::SaveAutomation,
    state::MachineContext,
    types::{Artifact, TableSource},
};
use oxy::config::model::{
    ExecuteSQLTask, LookerQueryTask, RouteRetrievalConfig, SQL, SemanticQueryTask, Task, TaskType,
    Workflow,
};
use oxy::constants::{PROCEDURE_FILE_EXTENSION, PROCEDURE_SAVED_DIR};
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
/// Only Table artifacts (sourced from SQL, Semantic, or Looker queries) are converted.
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
                mode: Default::default(),
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
                mode: Default::default(),
            }),
            Artifact::Table {
                source: TableSource::Looker { task },
                ..
            } => Some(Task {
                name: format!("looker_query_{}", i + 1),
                task_type: TaskType::LookerQuery(LookerQueryTask {
                    integration: task.integration.clone(),
                    model: task.model.clone(),
                    explore: task.explore.clone(),
                    query: task.query.clone(),
                    export: task.export.clone(),
                }),
                cache: None,
                mode: Default::default(),
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
            tracing::warn!("No artifacts to save as procedure");
            state.add_message(
                "No steps were captured to save as a procedure. Execute some queries or analyses first."
                    .to_string(),
            );
            return Ok(());
        }

        let automation_name = {
            let slug = slugify!(&self.objective, separator = "_");
            if slug.is_empty() {
                format!("procedure_{}", uuid::Uuid::new_v4().simple())
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

        // Write procedure to file
        let procedure_dir = execution_context
            .project
            .config_manager
            .resolve_file(PROCEDURE_SAVED_DIR)
            .await?;
        let procedure_dir = PathBuf::from(procedure_dir);
        tokio::fs::create_dir_all(&procedure_dir)
            .await
            .map_err(|e| {
                OxyError::RuntimeError(format!("Failed to create procedure directory: {}", e))
            })?;
        let yaml = serde_yaml::to_string(&workflow)
            .map_err(|e| OxyError::RuntimeError(format!("Failed to serialize procedure: {}", e)))?;

        // Use create_new(true) to atomically find a unique path and write the file,
        // avoiding both blocking exists() calls and the TOCTOU race.
        let mut candidate_name = automation_name.clone();
        let mut counter = 2u32;
        let procedure_path = loop {
            let path =
                procedure_dir.join(format!("{}{}", candidate_name, PROCEDURE_FILE_EXTENSION));
            match tokio::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
                .await
            {
                Ok(mut file) => {
                    use tokio::io::AsyncWriteExt;
                    file.write_all(yaml.as_bytes()).await.map_err(|e| {
                        OxyError::RuntimeError(format!("Failed to write procedure file: {}", e))
                    })?;
                    break path;
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    candidate_name = format!("{}_{}", automation_name, counter);
                    counter += 1;
                }
                Err(e) => {
                    return Err(OxyError::RuntimeError(format!(
                        "Failed to create procedure file: {}",
                        e
                    )));
                }
            }
        };

        tracing::info!("Saved procedure to: {}", procedure_path.display());
        state.add_message(format!(
            "Procedure '{}' saved successfully to {}",
            automation_name,
            procedure_path.display()
        ));

        Ok(())
    }
}
