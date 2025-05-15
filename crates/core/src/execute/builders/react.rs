use crate::{
    errors::OxyError,
    execute::{Executable, ExecutionContext},
};

use super::wrap::Wrap;

pub struct ReasonActWrapper<A, RF, IF> {
    act: A,
    response_fold: RF,
    input_fold: IF,
    max_iterations: usize,
}

impl<A, RF, IF> ReasonActWrapper<A, RF, IF> {
    pub fn new(act: A, response_fold: RF, input_fold: IF, max_iterations: usize) -> Self {
        Self {
            act,
            response_fold,
            input_fold,
            max_iterations,
        }
    }
}

impl<E, A, RF, IF> Wrap<E> for ReasonActWrapper<A, RF, IF>
where
    A: Clone,
    RF: Clone,
    IF: Clone,
{
    type Wrapper = ReasonAct<A, E, RF, IF>;

    fn wrap(&self, inner: E) -> ReasonAct<A, E, RF, IF> {
        ReasonAct::new(
            self.act.clone(),
            inner,
            self.response_fold.clone(),
            self.input_fold.clone(),
            self.max_iterations,
        )
    }
}

pub struct ReasonAct<A, E, RF, IF> {
    act: A,
    inner: E,
    response_fold: RF,
    input_fold: IF,
    max_iterations: usize,
}

impl<A, E, RF, IF> ReasonAct<A, E, RF, IF> {
    pub fn new(act: A, inner: E, response_fold: RF, input_fold: IF, max_iterations: usize) -> Self {
        Self {
            act,
            inner,
            response_fold,
            input_fold,
            max_iterations,
        }
    }
}

#[async_trait::async_trait]
impl<I, A, E, RF, IF> Executable<I> for ReasonAct<A, E, RF, IF>
where
    A: Executable<E::Response, Response = Option<I>> + Send,
    E: Executable<I> + Send,
    RF: Fn(&E::Response, Option<&E::Response>) -> E::Response + Send,
    IF: Fn(&I, &I) -> I + Send,
    I: Clone + Send + 'static,
{
    type Response = E::Response;

    async fn execute(
        &mut self,
        execution_context: &ExecutionContext,
        input: I,
    ) -> Result<Self::Response, OxyError> {
        let origin_input = input.clone();
        let response = self.inner.execute(execution_context, input).await?;
        let mut iterations = 0;
        let mut final_response = (self.response_fold)(&response, None);
        let mut current_response = response;

        loop {
            if iterations >= self.max_iterations {
                Err(OxyError::RuntimeError("Max iterations reached".to_string()))?;
            }
            let act_response = self
                .act
                .execute(execution_context, current_response)
                .await?;

            match act_response {
                Some(new_input) => {
                    let current_input = (self.input_fold)(&origin_input, &new_input);
                    let new_response = self.inner.execute(execution_context, current_input).await?;
                    final_response = (self.response_fold)(&final_response, Some(&new_response));
                    current_response = new_response;
                }
                None => break,
            }
            iterations += 1;
        }
        tracing::debug!("Stopped after {} iterations", iterations);
        Ok(final_response)
    }
}
