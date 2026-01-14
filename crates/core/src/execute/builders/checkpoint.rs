use std::fmt::Debug;

use serde::{Serialize, de::DeserializeOwned};
use tracing::Instrument;

use crate::{
    adapters::checkpoint::{CheckpointBuilder, CheckpointData, RunInfo},
    errors::OxyError,
    execute::{
        Executable, ExecutionContext,
        writer::{BufWriter, Writer},
    },
};

use super::wrap::Wrap;

pub struct CheckpointRootWrapper;

impl<E> Wrap<E> for CheckpointRootWrapper {
    type Wrapper = CheckpointRoot<E>;

    fn wrap(&self, inner: E) -> Self::Wrapper {
        CheckpointRoot::new(inner)
    }
}

pub struct CheckpointRoot<E> {
    inner: E,
}

impl<E> CheckpointRoot<E> {
    pub fn new(inner: E) -> Self {
        CheckpointRoot { inner }
    }
}

impl<E> Clone for CheckpointRoot<E>
where
    E: Clone,
{
    fn clone(&self) -> Self {
        CheckpointRoot {
            inner: self.inner.clone(),
        }
    }
}

pub trait CheckpointRootId {
    fn run_info(&self) -> RunInfo;
}

pub trait CheckpointId {
    fn checkpoint_hash(&self) -> String;
    fn replay_id(&self) -> String;
    fn child_run_info(&self) -> Option<RunInfo>;
    fn loop_values(&self) -> Option<Vec<serde_json::Value>>;
}

#[async_trait::async_trait]
impl<I, E, R> Executable<I> for CheckpointRoot<E>
where
    E: Executable<I, Response = R> + Send + Sync,
    I: Debug + CheckpointRootId + Send + Sync + 'static,
    R: Serialize + Send + Clone,
{
    type Response = E::Response;

    async fn execute(
        &mut self,
        execution_context: &ExecutionContext,
        input: I,
    ) -> Result<Self::Response, OxyError> {
        let runs_manager =
            &execution_context
                .project
                .runs_manager
                .clone()
                .ok_or(OxyError::ConfigurationError(
                    "Runs manager is not configured for the project".to_string(),
                ))?;
        let manager = CheckpointBuilder::from_runs_manager(runs_manager).await?;
        let run_info = input.run_info();
        tracing::info!("Running with run info: {:?}", run_info);
        // Build new execution context with the new receiver and checkpoint manager
        let response = {
            let checkpoint_context = match &execution_context.checkpoint {
                Some(checkpoint) => checkpoint.nested(run_info.clone()),
                None => manager.new_context(run_info.clone(), execution_context.user_id.clone()),
            };
            let new_context = execution_context.with_checkpoint(checkpoint_context);

            self.inner.execute(&new_context, input).await
        }?;
        // Commit checkpoint with a success marker
        manager.write_success_marker(&run_info).await?;
        Ok(response)
    }
}

pub struct CheckpointWrapper;

impl<E> Wrap<E> for CheckpointWrapper {
    type Wrapper = Checkpoint<E>;

    fn wrap(&self, inner: E) -> Self::Wrapper {
        Checkpoint::new(inner)
    }
}

pub struct Checkpoint<E> {
    inner: E,
}

impl<E> Checkpoint<E> {
    pub fn new(inner: E) -> Self {
        Checkpoint { inner }
    }
}

impl<E> Clone for Checkpoint<E>
where
    E: Clone,
{
    fn clone(&self) -> Self {
        Checkpoint {
            inner: self.inner.clone(),
        }
    }
}

#[async_trait::async_trait]
impl<E, I, R> Executable<I> for Checkpoint<E>
where
    E: Executable<I, Response = R> + Send + Sync,
    I: CheckpointId + Debug + Send + Sync + 'static,
    R: Serialize + DeserializeOwned + Send + Clone,
{
    type Response = E::Response;

    async fn execute(
        &mut self,
        execution_context: &ExecutionContext,
        input: I,
    ) -> Result<Self::Response, OxyError> {
        if execution_context.checkpoint.is_none() {
            return self.inner.execute(execution_context, input).await;
        }
        let replay_id = input.replay_id();
        let execution_context = execution_context.with_checkpoint_ref(&replay_id);
        let checkpoint_hash = input.checkpoint_hash();
        let child_run_info = input.child_run_info();
        let loop_values = input.loop_values();
        match execution_context.read_checkpoint::<R, I>(&input).await {
            Ok(data) => match data.output {
                Some(output) => {
                    tracing::info!(
                        "Checkpoint found for replay_id: {}, hash: {}",
                        replay_id,
                        checkpoint_hash
                    );
                    for event in data.events {
                        execution_context.write(event).await?;
                    }
                    return Ok(output);
                }
                None => {
                    tracing::warn!(
                        "Checkpoint found but no output for replay_id: {}, hash: {}",
                        replay_id,
                        checkpoint_hash
                    );
                }
            },
            Err(e) => {
                tracing::warn!(
                    "Checkpoint not found for replay_id: {}, hash: {}\nError: {}",
                    replay_id,
                    checkpoint_hash,
                    e
                );
            }
        }
        let mut buf_writer = BufWriter::new();
        let writer = buf_writer.create_writer(None)?;
        let tx = execution_context.writer.clone();

        // Capture the current span to propagate trace context to the spawned task
        let current_span = tracing::Span::current();

        let handle = tokio::spawn(
            async move { buf_writer.write_and_copy(tx).await }.instrument(current_span),
        );
        let response = {
            let new_context = &execution_context.wrap_writer(writer);
            self.inner.execute(new_context, input).await
        };
        let events = handle.await??;
        let checkpoint_data: Option<CheckpointData<R>> = match &response {
            Ok(response) => {
                tracing::info!(
                    "Checkpoint created for replay_id: {}, hash: {}",
                    replay_id,
                    checkpoint_hash
                );
                Some(CheckpointData {
                    replay_id,
                    checkpoint_hash,
                    output: Some(response.clone()),
                    events,
                    run_info: child_run_info,
                    loop_values,
                })
            }
            Err(e) => {
                tracing::error!(
                    "Failed to execute task for replay_id: {}, hash: {}\nError: {}",
                    replay_id,
                    checkpoint_hash,
                    e
                );
                match child_run_info {
                    Some(run_info) => Some(CheckpointData {
                        replay_id,
                        checkpoint_hash,
                        output: None,
                        events: vec![],
                        run_info: Some(run_info),
                        loop_values,
                    }),
                    None => None,
                }
            }
        };
        if let Some(checkpoint_data) = checkpoint_data {
            execution_context.create_checkpoint(checkpoint_data).await?;
        }
        response
    }
}
