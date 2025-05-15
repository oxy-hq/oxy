use std::{fmt::Debug, hash::Hash};

use serde::{Serialize, de::DeserializeOwned};
use tokio::sync::mpsc::channel;

use crate::{
    adapters::checkpoint::{CheckpointData, CheckpointManager, RunInfo},
    errors::OxyError,
    execute::{Executable, ExecutionContext, types::Event, writer::BufWriter},
};

use super::wrap::Wrap;

pub struct CheckpointRootWrapper<S> {
    should_restore: S,
    manager: CheckpointManager,
}

impl<S> CheckpointRootWrapper<S> {
    pub fn new(manager: CheckpointManager, should_restore: S) -> Self {
        CheckpointRootWrapper {
            manager,
            should_restore,
        }
    }
}

impl<E, S> Wrap<E> for CheckpointRootWrapper<S>
where
    S: Clone,
{
    type Wrapper = CheckpointRoot<E, S>;

    fn wrap(&self, inner: E) -> Self::Wrapper {
        CheckpointRoot::new(inner, self.manager.clone(), self.should_restore.clone())
    }
}

pub struct CheckpointRoot<E, S> {
    inner: E,
    manager: CheckpointManager,
    should_restore: S,
}

impl<E, S> CheckpointRoot<E, S> {
    pub fn new(inner: E, manager: CheckpointManager, should_restore: S) -> Self {
        CheckpointRoot {
            inner,
            manager,
            should_restore,
        }
    }
}

impl<E, S> Clone for CheckpointRoot<E, S>
where
    E: Clone,
    S: Clone,
{
    fn clone(&self) -> Self {
        CheckpointRoot {
            inner: self.inner.clone(),
            manager: self.manager.clone(),
            should_restore: self.should_restore.clone(),
        }
    }
}

#[async_trait::async_trait]
pub trait ShouldRestore {
    async fn check<I: Debug + Hash + Sync>(
        &self,
        input: &I,
        execution_context: &ExecutionContext,
        manager: &CheckpointManager,
    ) -> Option<RunInfo>;
}

#[derive(Clone)]
pub struct NoRestore;

#[async_trait::async_trait]
impl ShouldRestore for NoRestore {
    async fn check<I: Debug + Hash + Sync>(
        &self,
        _input: &I,
        _execution_context: &ExecutionContext,
        _manager: &CheckpointManager,
    ) -> Option<RunInfo> {
        None
    }
}

#[derive(Clone)]
pub struct LastRunFailed;

#[async_trait::async_trait]
impl ShouldRestore for LastRunFailed {
    async fn check<I: Debug + Hash + Sync>(
        &self,
        input: &I,
        _execution_context: &ExecutionContext,
        manager: &CheckpointManager,
    ) -> Option<RunInfo> {
        tracing::info!("Checking last run failed {:?}", input);
        let checkpoint_id = manager.checkpoint_id(input);
        let run_info = manager.last_run(&checkpoint_id).await.ok()?;
        if run_info.success {
            return None;
        }
        Some(run_info)
    }
}

#[async_trait::async_trait]
impl<I, E, S, R> Executable<I> for CheckpointRoot<E, S>
where
    E: Executable<I, Response = R> + Send + Sync,
    S: ShouldRestore + Send + Sync,
    I: Debug + Hash + Send + Sync + 'static,
    R: Serialize + Send + Clone,
{
    type Response = E::Response;

    async fn execute(
        &mut self,
        execution_context: &ExecutionContext,
        input: I,
    ) -> Result<Self::Response, OxyError> {
        // Check & Restore events
        let run_info = match self
            .should_restore
            .check(&input, execution_context, &self.manager)
            .await
        {
            Some(run) => {
                tracing::info!("Restoring run: {:?}", run);
                self.manager.read_events(&run, execution_context).await?;
                run
            }
            None => {
                let checkpoint_id = self.manager.checkpoint_id(&input);
                tracing::info!(
                    "Creating new run for input: {:?}\ncheckpoint_id: {}",
                    input,
                    checkpoint_id
                );
                self.manager.create_run(&checkpoint_id).await?
            }
        };
        tracing::info!("Running with checkpoint: {:?}", run_info);

        // Spawn event receiver task
        let (tx, rx) = channel::<Vec<Event>>(100);
        let manager = self.manager.clone();
        let run_info_clone = run_info.clone();
        let handle = tokio::spawn(async move {
            manager.write_events(&run_info_clone, rx).await?;
            Ok::<(), OxyError>(())
        });
        // Build new execution context with the new receiver and checkpoint manager
        let response = {
            let checkpoint_context = self.manager.new_context(run_info.clone(), tx);
            let new_context = execution_context.with_checkpoint(checkpoint_context);

            self.inner.execute(&new_context, input).await
        }?;
        // Commit checkpoint with a success marker
        handle.await??;
        self.manager.write_success_marker(&run_info).await?;
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
    I: Hash + Send + Sync + 'static,
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
        let checkpoint = execution_context.checkpoint.as_ref().unwrap();
        let checkpoint_id = checkpoint.checkpoint_id(&input);
        if let Ok(data) = checkpoint.read_checkpoint::<R>(&checkpoint_id).await {
            return Ok(data.output);
        }
        let mut buf_writer = BufWriter::new();
        let writer = buf_writer.create_writer(None)?;
        let tx = execution_context.writer.clone();
        let handle = tokio::spawn(async move { buf_writer.write_and_copy(tx).await });
        let response = {
            let new_context = execution_context.wrap_writer(writer);
            self.inner.execute(&new_context, input).await
        }?;
        let events = handle.await??;
        checkpoint
            .create_checkpoint(CheckpointData {
                checkpoint_id,
                output: response.clone(),
            })
            .await?;
        checkpoint.write_events(events).await?;
        Ok(response)
    }
}
