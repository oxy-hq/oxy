use std::marker::PhantomData;

use async_stream::stream;
use futures::StreamExt;
use tokio::task::JoinHandle;

use crate::{
    config::constants::{CONCURRENCY_ITEM_ID_PREFIX, CONCURRENCY_SOURCE},
    errors::OxyError,
    execute::{Executable, ExecutionContext, types::ProgressType, writer::OrderedWriter},
};

use super::wrap::Wrap;

pub struct ConcurrencyWrapper<T, C> {
    max: usize,
    concurrency_control: C,
    _input: PhantomData<T>,
}

impl<T> ConcurrencyWrapper<T, DefaultControl> {
    pub fn new(max: usize) -> Self {
        Self {
            max,
            concurrency_control: DefaultControl,
            _input: PhantomData,
        }
    }
}

impl<T, C> ConcurrencyWrapper<T, C> {
    pub fn with_concurrency_control(max: usize, concurrency_control: C) -> Self {
        ConcurrencyWrapper {
            max,
            concurrency_control,
            _input: PhantomData,
        }
    }
}

impl<E, T, C> Wrap<E> for ConcurrencyWrapper<T, C>
where
    C: Clone,
{
    type Wrapper = Concurrency<E, T, C>;

    fn wrap(&self, inner: E) -> Self::Wrapper {
        Concurrency::new(inner, self.max, self.concurrency_control.clone())
    }
}

pub struct Concurrency<E, T, C> {
    inner: E,
    max: usize,
    concurrency_control: C,
    _input: PhantomData<T>,
}

impl<E, T, C> Concurrency<E, T, C> {
    pub fn new(inner: E, max: usize, concurrency_control: C) -> Self {
        Self {
            inner,
            max,
            concurrency_control,
            _input: PhantomData,
        }
    }
}

#[async_trait::async_trait]
pub trait ConcurrencyControl<T> {
    type Response;

    async fn handle(
        &self,
        execution_context: &ExecutionContext,
        results_handle: JoinHandle<Result<Vec<Result<T, OxyError>>, OxyError>>,
        ordered_writer: OrderedWriter,
    ) -> Result<Self::Response, OxyError>;
}

#[derive(Clone)]
pub struct DefaultControl;

#[async_trait::async_trait]
impl<T> ConcurrencyControl<T> for DefaultControl
where
    T: Send + 'static,
{
    type Response = Vec<Result<T, OxyError>>;
    async fn handle(
        &self,
        execution_context: &ExecutionContext,
        results_handle: JoinHandle<Result<Vec<Result<T, OxyError>>, OxyError>>,
        ordered_writer: OrderedWriter,
    ) -> Result<Self::Response, OxyError> {
        let sender = execution_context.writer.clone();
        let events_handle = tokio::spawn(async move { ordered_writer.write_sender(sender).await });
        let results = results_handle.await??;
        events_handle.await??;
        Ok(results)
    }
}

#[async_trait::async_trait]
impl<E, I, T, C> Executable<I> for Concurrency<E, T, C>
where
    E: Executable<T> + Clone + Send + 'static,
    I: IntoIterator<Item = T> + Send + 'static,
    T: Clone + Send + 'static,
    C: ConcurrencyControl<E::Response> + Clone + Send,
    <C as ConcurrencyControl<<E as Executable<T>>::Response>>::Response: std::marker::Send,
{
    type Response = C::Response;

    async fn execute(
        &mut self,
        execution_context: &ExecutionContext,
        input: I,
    ) -> Result<Self::Response, OxyError> {
        let buffered = self.max;
        let params = input.into_iter().collect::<Vec<_>>();
        let total = params.len();
        execution_context
            .write_progress(ProgressType::Started(Some(total)))
            .await?;
        let mut ordered_writer = OrderedWriter::new();
        let pairs = params
            .into_iter()
            .enumerate()
            .map(|(idx, param)| {
                (
                    param,
                    self.inner.clone(),
                    execution_context.clone(),
                    execution_context
                        .wrap_writer(ordered_writer.create_writer(None))
                        .with_child_source(
                            format!("{}{}", CONCURRENCY_ITEM_ID_PREFIX, idx),
                            CONCURRENCY_SOURCE.to_string(),
                        ),
                )
            })
            .collect::<Vec<_>>();
        let stream = stream! {
            for (param, mut executable, concurrency_context, entry_context) in pairs.into_iter() {
                yield async move {
                  let output = executable.execute(&entry_context, param).await;
                  concurrency_context.write_progress(ProgressType::Updated(1)).await?;
                  output
                };
            }
        };
        let finish_context = execution_context.clone();
        let results_handle = tokio::spawn(async move {
            let result = stream.buffered(buffered).collect::<Vec<_>>().await;
            finish_context
                .write_progress(ProgressType::Finished)
                .await?;
            tracing::debug!("Concurrency sending back results");
            Ok(result)
        });
        let results = self
            .concurrency_control
            .handle(execution_context, results_handle, ordered_writer)
            .await?;

        Ok(results)
    }
}
