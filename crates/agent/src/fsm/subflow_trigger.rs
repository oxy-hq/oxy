use crate::fsm::state::MachineContext;
use oxy::execute::{ExecutionContext, builders::fsm::Trigger};
use oxy_shared::errors::OxyError;

#[allow(dead_code)]
pub struct SubflowRun<S> {
    pub context_id: String,
    pub objective: String,
    pub src: String,
    pub _state: std::marker::PhantomData<S>,
}

#[async_trait::async_trait]
impl Trigger for SubflowRun<MachineContext> {
    type State = MachineContext;

    async fn run(
        &self,
        _execution_context: &ExecutionContext,
        _current_state: &mut Self::State,
    ) -> Result<(), OxyError> {
        todo!("Implement SubflowRun trigger");
    }
}
