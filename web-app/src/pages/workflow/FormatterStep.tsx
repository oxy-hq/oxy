import useWorkflow, { TaskConfigWithId } from "@/stores/useWorkflow";
import { StepContainer } from "./StepContainer";
import { StepHeader } from "./StepHeader";

type Props = {
  task: TaskConfigWithId;
};

export function FormatterStep({ task }: Props) {
  const selectedNodeId = useWorkflow((state) => state.selectedNodeId);
  const selected = selectedNodeId === task.id;
  return (
    <StepContainer selected={selected}>
      <StepHeader task={task} />
    </StepContainer>
  );
}
