use crate::{
    errors::OxyError,
    execute::context::{Executable, ExecutionContext},
};

use super::wrap::Wrap;

#[async_trait::async_trait]
pub trait ContextMapper<I, V> {
    async fn map(
        &self,
        execution_context: &ExecutionContext,
        memo: V,
        input: I,
        value: V,
    ) -> Result<(V, Option<ExecutionContext>), OxyError>;
}

#[derive(Clone)]
pub struct NoopMapper;

#[async_trait::async_trait]
impl<I, V> ContextMapper<I, V> for NoopMapper
where
    I: Send + 'static,
    V: Send + 'static,
{
    async fn map(
        &self,
        _execution_context: &ExecutionContext,
        _memo: V,
        _input: I,
        value: V,
    ) -> Result<(V, Option<ExecutionContext>), OxyError> {
        Ok((value, None))
    }
}

pub struct ChainWrapper<M> {
    mapper: M,
}

impl<M> ChainWrapper<M> {
    pub fn new(mapper: M) -> Self {
        Self { mapper }
    }
}

impl<E, M> Wrap<E> for ChainWrapper<M>
where
    M: Clone,
{
    type Wrapper = Chain<E, M>;

    fn wrap(&self, inner: E) -> Chain<E, M> {
        Chain::new(inner, self.mapper.clone())
    }
}

pub struct Chain<E, M> {
    inner: E,
    mapper: M,
}

impl<E, M> Chain<E, M> {
    pub fn new(inner: E, mapper: M) -> Self {
        Self { inner, mapper }
    }
}

impl<E, M> Clone for Chain<E, M>
where
    E: Clone,
    M: Clone,
{
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            mapper: self.mapper.clone(),
        }
    }
}

#[async_trait::async_trait]
impl<IT, I, E, M, V> Executable<(IT, V)> for Chain<E, M>
where
    IT: IntoIterator<Item = I> + Send + 'static,
    I: Clone + Send + 'static,
    E: Executable<I, Response = V> + Send,
    M: ContextMapper<I, V> + Send,
    V: Clone + Send + 'static,
{
    type Response = E::Response;

    async fn execute(
        &mut self,
        execution_context: &ExecutionContext,
        input: (IT, V),
    ) -> Result<Self::Response, OxyError> {
        let (items, mut memo) = input;
        let mut execution_context = execution_context.clone();
        for item in items.into_iter().collect::<Vec<_>>() {
            let input = item.clone();
            let output = self.inner.execute(&execution_context, item).await?;
            let (new_output, new_context) = self
                .mapper
                .map(&execution_context, memo, input, output)
                .await?;
            if let Some(new_context) = new_context {
                execution_context = new_context;
            }
            memo = new_output;
        }
        Ok(memo)
    }
}
