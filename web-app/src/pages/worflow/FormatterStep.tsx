import { StepData} from ".";
import { StepContainer } from "./StepContainer";
import { StepHeader } from "./StepHeader";


type Props = {
  step: StepData;
};

export function FormatterStep({ step }: Props) {
  return (
    <StepContainer>
      <StepHeader step={step} />
    </StepContainer>
  );
}
