import { StepData } from ".";
import { AgentStep } from "./AgentStep";
import { FormatterStep } from "./FormatterStep";
import { ExecuteSqlStep } from "./ExecuteSqlStep";
import { LoopSequentialStep } from "./LoopSequentialStep";

type Props = {
  step: StepData;
};

export function StepItem({ step }: Props) {
  if (step.type === "loop_sequential") {
    return <LoopSequentialStep step={step} />;
  }
  if (step.type === "execute_sql") {
    return <ExecuteSqlStep step={step} />;
  }
  if (step.type === "formatter") {
    return <FormatterStep step={step} />;
  }
  if (step.type === "agent") {
    return <AgentStep step={step} />;
  }
}

