use crate::{
    errors::OxyError,
    execute::{Executable, ExecutionContext},
};

#[derive(Debug, Clone)]
pub struct FSM<M, S> {
    pub machine: M,
    _state: std::marker::PhantomData<S>,
}

impl<M, S> FSM<M, S> {
    pub fn new(machine: M) -> Self {
        Self {
            machine,
            _state: std::marker::PhantomData,
        }
    }
}

#[async_trait::async_trait]
pub trait State: Sized {
    type Machine;

    async fn first_trigger(
        &mut self,
        execution_context: &ExecutionContext,
        machine: &mut Self::Machine,
    ) -> Result<Box<dyn Trigger<State = Self>>, OxyError>;

    async fn next_trigger(
        &mut self,
        execution_context: &ExecutionContext,
        machine: &mut Self::Machine,
    ) -> Result<Option<Box<dyn Trigger<State = Self>>>, OxyError>;

    async fn handle_error(
        self,
        execution_context: &ExecutionContext,
        machine: &mut Self::Machine,
        error: OxyError,
    ) -> Result<Self, OxyError>;
}

#[async_trait::async_trait]
pub trait Machine<I> {
    type State;

    async fn wrap_context(
        &self,
        execution_context: &ExecutionContext,
    ) -> Result<ExecutionContext, OxyError> {
        Ok(execution_context.clone())
    }
    async fn start(
        &mut self,
        execution_context: &ExecutionContext,
        input: I,
    ) -> Result<Self::State, OxyError>;
    async fn end(
        &mut self,
        execution_context: &ExecutionContext,
        final_state: Self::State,
    ) -> Result<Self::State, OxyError>;
}

#[async_trait::async_trait]
pub trait Trigger: Send + Sync {
    type State;

    async fn run(
        &self,
        execution_context: &ExecutionContext,
        current_state: &mut Self::State,
    ) -> Result<(), OxyError>;
}

#[async_trait::async_trait]
impl<T> Trigger for Box<T>
where
    T: Trigger + ?Sized + Sync,
    T::State: Send,
{
    type State = T::State;

    async fn run(
        &self,
        execution_context: &ExecutionContext,
        current_state: &mut Self::State,
    ) -> Result<(), OxyError> {
        (**self).run(execution_context, current_state).await
    }
}

#[async_trait::async_trait]
impl<I, M, S> Executable<I> for FSM<M, S>
where
    S: State<Machine = M> + Send + 'static,
    M: Machine<I, State = S> + Send + Sync,
    I: Send + 'static,
{
    type Response = M::State;

    async fn execute(
        &mut self,
        execution_context: &ExecutionContext,
        input: I,
    ) -> Result<Self::Response, OxyError> {
        let execution_context = self.machine.wrap_context(execution_context).await?;
        let machine = &mut self.machine;
        let mut state = machine.start(&execution_context, input).await?;
        let init_trigger = state.first_trigger(&execution_context, machine).await?;
        init_trigger.run(&execution_context, &mut state).await?;

        loop {
            let next = state.next_trigger(&execution_context, machine).await?;
            if let Some(trigger) = next {
                if let Err(err) = trigger.run(&execution_context, &mut state).await {
                    state = state.handle_error(&execution_context, machine, err).await?;
                }
            } else {
                break;
            };
        }
        machine.end(&execution_context, state).await
    }
}
