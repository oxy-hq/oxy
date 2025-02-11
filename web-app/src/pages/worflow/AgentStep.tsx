import { StepData } from ".";
import { StepContainer } from "./StepContainer";
import { StepHeader } from "./StepHeader";

type Props = {
  step: StepData;
};

export function AgentStep({ step }: Props) {
  return (
    <StepContainer>
      <StepHeader step={step} />
    </StepContainer>
  );
}
