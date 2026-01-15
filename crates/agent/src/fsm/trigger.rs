use oxy::execute::{ExecutionContext, builders::fsm::Trigger, types::event::Step};
use oxy_shared::errors::OxyError;

pub struct StepTrigger<S> {
    pub step: Step,
    pub trigger: Box<dyn Trigger<State = S>>,
}

impl<S> StepTrigger<S>
where
    S: 'static + Send + std::fmt::Debug,
{
    pub fn boxed(step: Step, trigger: Box<dyn Trigger<State = S>>) -> Box<dyn Trigger<State = S>> {
        Box::new(Self { step, trigger })
    }
}

#[async_trait::async_trait]
impl<S> Trigger for StepTrigger<S>
where
    S: Send + std::fmt::Debug,
{
    type State = S;

    async fn run(
        &self,
        execution_context: &ExecutionContext,
        state: &mut Self::State,
    ) -> Result<(), OxyError> {
        execution_context
            .write_step_started(self.step.clone())
            .await?;
        tracing::info!(
            "Starting step: {:?} with objective: {:?}",
            self.step.kind,
            self.step.objective
        );
        let response = self.trigger.run(execution_context, state).await;
        tracing::info!(
            "Finished step: {:?} with result: {:?}",
            self.step.kind,
            response
        );
        execution_context
            .write_step_finished(
                self.step.id.clone(),
                response.as_ref().err().map(|e| e.to_string()),
            )
            .await?;
        response
    }
}
