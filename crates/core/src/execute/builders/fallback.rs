use crate::{
    errors::OxyError,
    execute::{
        Executable, ExecutionContext,
        types::Event,
        writer::{BufWriter, Writer},
    },
};

use super::wrap::Wrap;

pub struct FallbackWrapper<C, F, P> {
    condition_fn: C,
    fallback: F,
    event_predict: P,
}

impl<C, F, P> FallbackWrapper<C, F, P> {
    pub fn new(condition_fn: C, fallback: F, event_predict: P) -> Self {
        Self {
            condition_fn,
            fallback,
            event_predict,
        }
    }
}

impl<C, F, P> Clone for FallbackWrapper<C, F, P>
where
    C: Clone,
    F: Clone,
    P: Clone,
{
    fn clone(&self) -> Self {
        FallbackWrapper {
            condition_fn: self.condition_fn.clone(),
            fallback: self.fallback.clone(),
            event_predict: self.event_predict.clone(),
        }
    }
}

impl<E, C, F, P> Wrap<E> for FallbackWrapper<C, F, P>
where
    C: Clone,
    F: Clone,
    P: Clone,
{
    type Wrapper = Fallback<E, C, F, P>;

    fn wrap(&self, inner: E) -> Self::Wrapper {
        Fallback {
            inner,
            condition_fn: self.condition_fn.clone(),
            event_predict: self.event_predict.clone(),
            fallback: self.fallback.clone(),
        }
    }
}

pub struct Fallback<E, C, F, P> {
    inner: E,
    condition_fn: C,
    event_predict: P,
    fallback: F,
}

#[async_trait::async_trait]
impl<I, E, C, P, F> Executable<I> for Fallback<E, C, F, P>
where
    I: Clone + Send + 'static,
    E: Executable<I> + Send,
    E::Response: Send + 'static,
    C: Fn(&E::Response) -> bool + Send,
    P: Fn(&Event) -> bool + Clone + Send + Sync + 'static,
    F: Executable<I, Response = E::Response> + Send,
{
    type Response = E::Response;

    async fn execute(
        &mut self,
        execution_context: &ExecutionContext,
        input: I,
    ) -> Result<Self::Response, OxyError> {
        let mut buf_writer = BufWriter::new();
        let event_predict = self.event_predict.clone();
        let original_writer = execution_context.writer.clone();
        let writer = buf_writer.create_writer(None)?;
        let event_handle =
            tokio::spawn(
                async move { buf_writer.write_when(original_writer, event_predict).await },
            );
        let response = {
            let fallback_context = execution_context.wrap_writer(writer);
            self.inner.execute(&fallback_context, input.clone()).await
        }?;

        let events = event_handle.await??;
        // If the condition is met, write the response to the original writer else fallback
        // and return the fallback response.
        if (self.condition_fn)(&response) {
            // Write remaining events if any
            for event in events {
                execution_context.write(event).await?;
            }
            Ok(response)
        } else {
            self.fallback.execute(execution_context, input).await
        }
    }
}
