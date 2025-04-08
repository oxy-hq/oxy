import useWorkflow, { TaskConfigWithId } from "@/stores/useWorkflow";
import { StepContainer } from "./StepContainer.tsx";
import { NodeHeader } from "./NodeHeader.tsx";

type Props = {
  task: TaskConfigWithId;
};

export function FormatterNode({ task }: Props) {
  const selectedNodeId = useWorkflow((state) => state.selectedNodeId);
  const selected = selectedNodeId === task.id;
  return (
    <StepContainer selected={selected}>
      <NodeHeader name={task.name} type={task.type} />
    </StepContainer>
  );
}
