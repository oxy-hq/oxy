use std::{fmt::Debug, sync::OnceLock};

use serde::{Serialize, de::DeserializeOwned};
use tokio::sync::mpsc;
use tracing::Instrument;

use crate::{
    checkpoint::{CheckpointBuilder, CheckpointData, CheckpointStorage, RunInfo},
    execute::{
        Executable, ExecutionContext,
        writer::{BufWriter, Writer},
    },
};
use oxy_shared::errors::OxyError;

use super::wrap::Wrap;

// Best-effort background checkpoint persistence. Execution should never wait
// on checkpoint writes; when the queue is full we drop the checkpoint.
const CHECKPOINT_WRITE_QUEUE_SIZE: usize = 10000;
// Maximum number of checkpoints to batch into a single DB write.
const CHECKPOINT_BATCH_SIZE: usize = 64;
static CHECKPOINT_WRITE_QUEUE: OnceLock<mpsc::Sender<QueuedCheckpoint>> = OnceLock::new();

#[derive(Clone)]
struct QueuedCheckpoint {
    storage: crate::checkpoint::CheckpointStorageImpl,
    run_info: RunInfo,
    checkpoint: CheckpointData<serde_json::Value>,
}

fn checkpoint_write_queue() -> &'static mpsc::Sender<QueuedCheckpoint> {
    CHECKPOINT_WRITE_QUEUE.get_or_init(|| {
        let (tx, rx) = mpsc::channel(CHECKPOINT_WRITE_QUEUE_SIZE);
        tokio::spawn(async move {
            run_checkpoint_write_worker(rx).await;
            tracing::error!(
                "Checkpoint write worker exited — checkpoints will be dropped for this process"
            );
        });
        tx
    })
}

async fn run_checkpoint_write_worker(mut receiver: mpsc::Receiver<QueuedCheckpoint>) {
    let mut batch: Vec<QueuedCheckpoint> = Vec::with_capacity(CHECKPOINT_BATCH_SIZE);
    loop {
        batch.clear();
        // Block until at least one item is available
        let n = receiver.recv_many(&mut batch, CHECKPOINT_BATCH_SIZE).await;
        if n == 0 {
            break; // channel closed
        }
        // All checkpoints in a process share the same storage instance
        let storage = batch[0].storage.clone();
        let items: Vec<(RunInfo, CheckpointData<serde_json::Value>)> = batch
            .drain(..)
            .map(|q| (q.run_info, q.checkpoint))
            .collect();
        if let Err(e) = storage.create_checkpoints_batch(items).await {
            tracing::warn!("Failed to write checkpoint batch (non-fatal): {}", e);
        }
    }
}

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
        let runs_manager = &execution_context.workspace.runs_manager.clone().ok_or(
            OxyError::ConfigurationError(
                "Runs manager is not configured for the project".to_string(),
            ),
        )?;
        let manager = CheckpointBuilder::from_runs_manager(runs_manager).await?;
        let run_info = input.run_info();
        tracing::info!("Running with run info: {:?}", run_info);
        // Build new execution context with the new receiver and checkpoint manager
        let response = {
            let checkpoint_context = match &execution_context.checkpoint {
                Some(checkpoint) => checkpoint.nested(run_info.clone()),
                None => manager.new_context(run_info.clone(), execution_context.user_id),
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
                tracing::debug!(
                    "Checkpoint not found for replay_id: {}, hash: {}, error: {}",
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
            match checkpoint_data.into_value() {
                Ok(mut json_checkpoint) => {
                    if let Some(checkpoint_context) = execution_context.checkpoint.as_ref() {
                        // Pre-resolve replay_id here so the worker only needs to write
                        json_checkpoint.replay_id = checkpoint_context.current_ref_str();
                        let queued_checkpoint = QueuedCheckpoint {
                            storage: checkpoint_context.storage().clone(),
                            run_info: checkpoint_context.run_info().clone(),
                            checkpoint: json_checkpoint,
                        };
                        if let Err(e) = checkpoint_write_queue().try_send(queued_checkpoint) {
                            tracing::warn!(
                                "Checkpoint queue is full or unavailable; dropping checkpoint (non-fatal): {}",
                                e
                            );
                        }
                    } else {
                        tracing::warn!(
                            "Checkpoint context missing; dropping checkpoint (non-fatal)"
                        );
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to serialize checkpoint (non-fatal): {}", e);
                }
            }
        }
        response
    }
}
