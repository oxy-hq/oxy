use tokio::task::JoinHandle;

use crate::{
    errors::OxyError,
    execute::{Executable, ExecutionContext, writer::BufWriter},
};

use super::wrap::Wrap;

#[async_trait::async_trait]
pub trait Exporter<I, O> {
    async fn should_export(&self, execution_context: &ExecutionContext, input: &I) -> bool;
    async fn export(
        &self,
        execution_context: &ExecutionContext,
        buf_writer: BufWriter,
        input: I,
        output: JoinHandle<Result<O, OxyError>>,
    ) -> Result<O, OxyError>;
}

pub struct ExportWrapper<W> {
    exporter: W,
}

impl<W> ExportWrapper<W> {
    pub fn new(exporter: W) -> Self {
        Self { exporter }
    }
}

impl<E, W> Wrap<E> for ExportWrapper<W>
where
    W: Clone,
{
    type Wrapper = Export<E, W>;

    fn wrap(&self, inner: E) -> Export<E, W> {
        Export::new(inner, self.exporter.clone())
    }
}

pub struct Export<E, W> {
    exporter: W,
    inner: E,
}

impl<E, W> Export<E, W> {
    pub fn new(inner: E, exporter: W) -> Self {
        Export { inner, exporter }
    }
}

impl<E, W> Clone for Export<E, W>
where
    E: Clone,
    W: Clone,
{
    fn clone(&self) -> Self {
        Export {
            exporter: self.exporter.clone(),
            inner: self.inner.clone(),
        }
    }
}

#[async_trait::async_trait]
impl<I, E, W> Executable<I> for Export<E, W>
where
    E: Executable<I> + Clone + Send + 'static,
    I: Clone + Send + 'static,
    W: Exporter<I, E::Response> + Send,
{
    type Response = E::Response;

    async fn execute(
        &mut self,
        execution_context: &ExecutionContext,
        input: I,
    ) -> Result<Self::Response, OxyError> {
        if !self.exporter.should_export(execution_context, &input).await {
            return self.inner.execute(execution_context, input).await;
        }

        let mut buf_writer = BufWriter::new();
        let writer = buf_writer.create_writer(None)?;
        let export_context = execution_context.wrap_writer(writer);
        let mut executable = self.inner.clone();
        let input_clone = input.clone();
        let output_handle =
            tokio::spawn(async move { executable.execute(&export_context, input_clone).await });
        self.exporter
            .export(execution_context, buf_writer, input, output_handle)
            .await
    }
}
