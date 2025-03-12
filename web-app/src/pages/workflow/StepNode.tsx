import { Handle, NodeProps, Position } from "@xyflow/react";

import { NodeContainer } from "./NodeContainer";
import { StepItem } from "./StepItem";
import useWorkflow, { NodeData } from "@/stores/useWorkflow";

type NodeType = {
  id: string;
  data: NodeData;
  position: {
    x: number;
    y: number;
  };
  type: string;
};

type Props = NodeProps<NodeType>;

export function StepNode({ data, isConnectable }: Props) {
  const selectedNodeId = useWorkflow((state) => state.selectedNodeId);
  const selected = selectedNodeId === data.id;
  return (
    <div key={data.id}>
      <NodeContainer selected={selected}>
        <Handle
          type="target"
          position={Position.Top}
          isConnectable={isConnectable}
          className="invisible top-0.75"
        />
        <StepItem task={data.task} />
        <Handle
          type="source"
          position={Position.Bottom}
          isConnectable={isConnectable}
          className="invisible bottom-0.25"
        />
      </NodeContainer>
    </div>
  );
}
