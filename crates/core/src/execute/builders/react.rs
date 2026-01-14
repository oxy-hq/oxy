use crate::{
    errors::OxyError,
    execute::{Executable, ExecutionContext},
};

use super::wrap::Wrap;

pub struct ReasonActWrapper<A> {
    act: A,
    strategy: IterationStrategy,
}

impl<A> ReasonActWrapper<A> {
    pub fn new(act: A, strategy: IterationStrategy) -> Self {
        Self { act, strategy }
    }
}

#[derive(Debug, Clone)]
pub enum IterationStrategy {
    Exhaustive { max_iterations: usize },
    RAR, // Reason Act Reason
    Once,
}

pub enum Decision {
    Continue,
    Break,
    BreakInNextReasoning,
    Error(OxyError),
}

impl IterationStrategy {
    pub fn should_break(&self, iterations: usize) -> Decision {
        match self {
            IterationStrategy::Exhaustive { max_iterations } => {
                if iterations >= *max_iterations {
                    Decision::Error(OxyError::RuntimeError("Max iterations reached".to_string()))
                } else {
                    Decision::Continue
                }
            }
            IterationStrategy::RAR => {
                if iterations > 0 {
                    Decision::BreakInNextReasoning
                } else {
                    Decision::Continue
                }
            }
            IterationStrategy::Once => {
                if iterations > 0 {
                    Decision::Break
                } else {
                    Decision::Continue
                }
            }
        }
    }
}

impl<E, A> Wrap<E> for ReasonActWrapper<A>
where
    A: Clone,
{
    type Wrapper = ReasonAct<A, E>;

    fn wrap(&self, inner: E) -> ReasonAct<A, E> {
        ReasonAct::new(self.act.clone(), inner, self.strategy.clone())
    }
}

pub struct ReasonAct<A, E> {
    act: A,
    inner: E,
    strategy: IterationStrategy,
}

impl<A, E> ReasonAct<A, E> {
    pub fn new(act: A, inner: E, strategy: IterationStrategy) -> Self {
        Self {
            act,
            inner,
            strategy,
        }
    }
}

impl<A, E> Clone for ReasonAct<A, E>
where
    A: Clone,
    E: Clone,
{
    fn clone(&self) -> Self {
        Self {
            act: self.act.clone(),
            inner: self.inner.clone(),
            strategy: self.strategy.clone(),
        }
    }
}

#[async_trait::async_trait]
impl<I, A, E, R> Executable<I> for ReasonAct<A, E>
where
    A: Executable<E::Response, Response = Option<I>> + Send,
    E: Executable<I, Response = R> + Send,
    I: Clone + Send + 'static,
    R: Clone + Send + 'static,
{
    type Response = Vec<E::Response>;

    async fn execute(
        &mut self,
        execution_context: &ExecutionContext,
        input: I,
    ) -> Result<Self::Response, OxyError> {
        let mut iterations = 0;
        let mut final_response: Vec<R> = vec![];
        let mut current_input = input;

        loop {
            match self.strategy.should_break(iterations) {
                Decision::Continue => {
                    tracing::debug!(react.iteration = iterations, "Continuing iteration");
                }
                Decision::Break => {
                    tracing::debug!(react.iteration = iterations, "Breaking ReAct loop");
                    break;
                }
                Decision::BreakInNextReasoning => {
                    tracing::debug!(react.iteration = iterations, "Final reasoning before break");
                    let response = self.inner.execute(execution_context, current_input).await?;
                    final_response.push(response.clone());
                    break;
                }
                Decision::Error(e) => {
                    tracing::error!(react.iteration = iterations, error = %e, "ReAct loop error");
                    return Err(e);
                }
            }

            tracing::debug!(react.iteration = iterations, "Executing reasoning step");
            let response = self.inner.execute(execution_context, current_input).await?;
            final_response.push(response.clone());

            tracing::debug!(react.iteration = iterations, "Executing action step");
            match self.act.execute(execution_context, response).await? {
                Some(new_input) => {
                    tracing::debug!(
                        react.iteration = iterations,
                        "Action produced new input, continuing loop"
                    );
                    current_input = new_input;
                }
                None => {
                    tracing::info!(
                        react.iteration = iterations,
                        "Action completed, ending loop"
                    );
                    break;
                }
            }
            iterations += 1;
        }

        tracing::info!(react.total_iterations = iterations, "ReAct loop completed");
        Ok(final_response)
    }
}
