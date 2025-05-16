import { NoneTaskNodeType } from "@/stores/useWorkflow";
import { NodeHeader } from "./NodeHeader";
import { StepContainer } from "./StepContainer";

type Props = {
  width?: number;
  height?: number;
};

export function ConditionalElseNode({ width, height }: Props) {
  return (
    <StepContainer width={width} height={height}>
      <NodeHeader name="" type={NoneTaskNodeType.CONDITIONAL_ELSE} />
    </StepContainer>
  );
}
