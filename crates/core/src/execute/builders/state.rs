use crate::{
    errors::OxyError,
    execute::context::{Executable, ExecutionContext},
};

use super::wrap::Wrap;

pub struct StateWrapper<S> {
    state: S,
}

impl<S> StateWrapper<S> {
    pub fn new(state: S) -> Self {
        Self { state }
    }
}

impl<E, S> Wrap<E> for StateWrapper<S>
where
    S: Clone,
{
    type Wrapper = State<E, S>;

    fn wrap(&self, inner: E) -> State<E, S> {
        State::new(inner, self.state.clone())
    }
}

pub struct State<E, S> {
    inner: E,
    state: S,
}

impl<E, S> Clone for State<E, S>
where
    S: Clone,
    E: Clone,
{
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            state: self.state.clone(),
        }
    }
}

impl<E, S> State<E, S> {
    pub fn new(inner: E, state: S) -> Self {
        Self { inner, state }
    }
}

#[async_trait::async_trait]
impl<E, S, I> Executable<I> for State<E, S>
where
    E: Executable<(S, I)> + Send,
    S: Clone + Send + 'static,
    I: Send + 'static,
{
    type Response = E::Response;

    async fn execute(
        &mut self,
        execution_context: &ExecutionContext,
        input: I,
    ) -> Result<Self::Response, OxyError> {
        self.inner
            .execute(execution_context, (self.state.clone(), input))
            .await
    }
}
