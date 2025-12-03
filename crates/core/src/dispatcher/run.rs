use uuid::Uuid;

use crate::{
    adapters::{
        checkpoint::types::RetryStrategy, project::manager::ProjectManager, runs::TopicRef,
    },
    errors::OxyError,
    execute::{types::OutputContainer, writer::Handler},
    service::{
        block::GroupBlockHandler,
        statics::BROADCASTER,
        task_manager::TASK_MANAGER,
        types::{event::EventKind, run::RunInfo},
    },
};

pub struct Dispatcher {
    pm: ProjectManager,
}

#[async_trait::async_trait]
pub trait Dispatch {
    async fn run(
        &self,
        project_manager: ProjectManager,
        topic_ref: TopicRef<EventKind>,
        source_id: String,
        retry_strategy: RetryStrategy,
    ) -> Result<OutputContainer, OxyError>;
}

impl Dispatcher {
    pub fn new(pm: ProjectManager) -> Self {
        Self { pm }
    }

    pub async fn dispatch<D: Dispatch + Send + 'static>(
        &self,
        source_id: String,
        retry_strategy: RetryStrategy,
        dispatch: D,
        lookup_id: Option<Uuid>,
    ) -> Result<RunInfo, OxyError> {
        // Dispatch logic here
        let runs_manager = self.pm.runs_manager.clone().ok_or_else(|| {
            tracing::error!("Failed to initialize RunsManager");
            OxyError::InitializationError(format!("Failed to initialize RunsManager"))
        })?;
        let (source_run_info, root_run_info) = runs_manager
            .get_root_run(&source_id, &retry_strategy, lookup_id)
            .await
            .map_err(|e| {
                tracing::error!("Failed to get run info: {:?}", e);
                OxyError::DBError(format!(
                    "Failed to get run info for source_id {}: {:?}",
                    source_id, e
                ))
            })?;
        let replay_id = retry_strategy.replay_id(&source_run_info.root_ref);
        let run_info = root_run_info.unwrap_or(source_run_info);
        let task_id = run_info.task_id()?;
        let topic_id = task_id.clone();
        let topic_ref = BROADCASTER.create_topic(&task_id).await.map_err(|err| {
            tracing::error!("Failed to create topic for task ID {task_id}: {err}");
            OxyError::RuntimeError(format!(
                "Failed to create topic for task ID {task_id}: {err}"
            ))
        })?;
        let run_index = run_info
            .run_index
            .ok_or(OxyError::ArgumentError(format!("Run index not available")))?;

        let callback = async move || -> Result<(), OxyError> {
            if let Some(closed) = BROADCASTER.remove_topic(&topic_id).await {
                let mut group_handler = GroupBlockHandler::new();
                for event in closed.items {
                    group_handler.handle_event(event).await?;
                }
                let groups = group_handler.collect();
                for group in groups {
                    tracing::info!("Saving group: {:?}", group.id());
                    runs_manager.upsert_run(group).await?;
                }
                drop(closed.sender); // Drop the sender to close the channel
            }
            Ok(())
        };
        let project_manager = self.pm.clone();
        TASK_MANAGER
            .spawn(task_id.clone(), async move |cancellation_token| {
                let run_fut = {
                    let converted_run_index = run_index
                        .try_into()
                        .map_err(|e| tracing::error!("Failed to convert run_index to u32: {}", e))
                        .unwrap_or(0); // Default to 0 if conversion fails
                    dispatch.run(
                        project_manager,
                        topic_ref,
                        source_id,
                        RetryStrategy::Retry {
                            replay_id,
                            run_index: converted_run_index,
                        },
                    )
                };
                tokio::select! {
                    _ = cancellation_token.cancelled() => {
                        tracing::info!("Task {task_id} was cancelled");
                        if let Err(err) = callback().await {
                            tracing::error!("Failed to handle callback for task {task_id}: {err}");
                        }
                    }
                    res = run_fut => {
                        let _output = match res {
                            Ok(value) => {
                                tracing::info!("Task {task_id} completed successfully");
                                Some(value)
                            },
                            Err(err) => {
                                tracing::error!("Task {task_id} failed: {err}");
                                None
                            },
                        };

                        if let Err(err) = callback().await {
                            tracing::error!("Failed to handle callback for task {task_id}: {err}");
                        }
                    }

                }
            })
            .await;
        Ok(run_info)
    }
}
