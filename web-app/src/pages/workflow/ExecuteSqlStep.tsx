import useWorkflow, { TaskConfigWithId } from "@/stores/useWorkflow";
import { StepContainer } from "./StepContainer";
import { TaskHeader } from "./TaskHeader.tsx";

type Props = {
  task: TaskConfigWithId;
};

export function ExecuteSqlStep({ task }: Props) {
  const selectedNodeId = useWorkflow((state) => state.selectedNodeId);
  const selected = selectedNodeId === task.id;
  return (
    <StepContainer selected={selected}>
      <TaskHeader task={task} />
    </StepContainer>
  );
}
