use tokio::task::JoinHandle;

use crate::{
    config::constants::{CONSISTENCY_SOURCE, CONSISTENCY_THRESHOLD},
    errors::OxyError,
    execute::{Executable, ExecutionContext, types::EventKind, writer::OrderedWriter},
};

use super::{
    concurrency::{Concurrency, ConcurrencyControl},
    wrap::Wrap,
};

pub struct ConsistencyWrapper<P> {
    consistency_picker: P,
    sample_size: usize,
    max_concurrency: usize,
}

impl<P> ConsistencyWrapper<P> {
    pub fn new(consistency_picker: P, sample_size: usize, max_concurrency: usize) -> Self {
        Self {
            consistency_picker,
            sample_size,
            max_concurrency,
        }
    }
}

impl<E, P> Wrap<E> for ConsistencyWrapper<P>
where
    P: Clone,
{
    type Wrapper = Consistency<E, P>;

    fn wrap(&self, inner: E) -> Consistency<E, P> {
        Consistency::new(
            inner,
            self.consistency_picker.clone(),
            self.sample_size,
            self.max_concurrency,
        )
    }
}

pub struct Consistency<E, P> {
    inner: E,
    consistency_picker: P,
    sample_size: usize,
    max_concurrency: usize,
}

impl<E, P> Consistency<E, P> {
    pub fn new(
        inner: E,
        consistency_picker: P,
        sample_size: usize,
        max_concurrency: usize,
    ) -> Self {
        Self {
            inner,
            consistency_picker,
            sample_size,
            max_concurrency,
        }
    }
}

impl<E, P> Clone for Consistency<E, P>
where
    E: Clone,
    P: Clone,
{
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            consistency_picker: self.consistency_picker.clone(),
            sample_size: self.sample_size,
            max_concurrency: self.max_concurrency,
        }
    }
}

pub struct ConsistencyControl<P> {
    picker: P,
}

impl<P> ConsistencyControl<P> {
    pub fn new(picker: P) -> Self {
        Self { picker }
    }
}
impl<P> Clone for ConsistencyControl<P>
where
    P: Clone,
{
    fn clone(&self) -> Self {
        Self {
            picker: self.picker.clone(),
        }
    }
}

#[async_trait::async_trait]
pub trait ConsistencyPicker<T> {
    async fn pick(
        &self,
        execution_context: &ExecutionContext,
        results: Vec<Result<T, OxyError>>,
    ) -> Result<(usize, T, f32), OxyError>;
}

#[async_trait::async_trait]
impl<T, P> ConcurrencyControl<T> for ConsistencyControl<P>
where
    P: ConsistencyPicker<T> + Sync,
    T: Send + 'static,
{
    type Response = (T, f32);

    async fn handle(
        &self,
        execution_context: &ExecutionContext,
        results_handle: JoinHandle<Result<Vec<Result<T, OxyError>>, OxyError>>,
        ordered_writer: OrderedWriter,
    ) -> Result<Self::Response, OxyError> {
        let results = results_handle.await??;
        let picker_context = execution_context.with_child_source(
            format!("{}-scores", execution_context.source.id),
            CONSISTENCY_SOURCE.to_string(),
        );
        execution_context
            .write_kind(EventKind::Message {
                message: "🔄Evaluating records".to_string(),
            })
            .await?;
        let (idx, picked_result, score) = self.picker.pick(&picker_context, results).await?;
        if score < CONSISTENCY_THRESHOLD {
            execution_context
                .write_kind(EventKind::Message {
                    message: format!(
                        "Warning: results for this step are not consistent. Try testing this step in isolation and reworking the prompt. Consistency: {:.2}%",
                        score * 100.0
                    ),
                })
                .await?;
        }
        ordered_writer
            .write_partition(idx, execution_context.writer.clone())
            .await?;
        Ok((picked_result, score))
    }
}

#[async_trait::async_trait]
impl<I, E, P> Executable<I> for Consistency<E, P>
where
    E: Executable<I> + Clone + Send + 'static,
    P: ConsistencyPicker<E::Response> + Clone + Send + Sync,
    I: Clone + Send + 'static,
{
    type Response = (E::Response, f32);

    async fn execute(
        &mut self,
        execution_context: &ExecutionContext,
        input: I,
    ) -> Result<Self::Response, OxyError> {
        let consistency_context = execution_context.with_child_source(
            format!(
                "{}-outputs-{}",
                CONSISTENCY_SOURCE, execution_context.source.id
            ),
            CONSISTENCY_SOURCE.to_string(),
        );
        let mut concurrency_executable = Concurrency::new(
            self.inner.clone(),
            self.max_concurrency,
            ConsistencyControl::new(self.consistency_picker.clone()),
        );
        let sample_size = self.sample_size;
        let input_clone = input.clone();
        execution_context
            .write_kind(EventKind::Message {
                message: "🔄Generating outputs".to_string(),
            })
            .await?;
        concurrency_executable
            .execute(
                &consistency_context,
                (0..sample_size)
                    .map(|_| input_clone.clone())
                    .collect::<Vec<_>>(),
            )
            .await
    }
}
