import { NoneTaskNodeType } from "@/stores/useWorkflow";
import { NodeHeader } from "./NodeHeader";
import { StepContainer } from "./StepContainer";

type Props = {
  width?: number;
  height?: number;
  condition: string;
};

export function ConditionalIfNode({ width, height, condition }: Props) {
  return (
    <StepContainer width={width} height={height}>
      <NodeHeader name={condition} type={NoneTaskNodeType.CONDITIONAL_IF} />
    </StepContainer>
  );
}
