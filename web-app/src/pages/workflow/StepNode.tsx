import { Handle, NodeProps, Position } from "@xyflow/react";

import { NodeContainer } from "./NodeContainer";
import { StepItem } from "./StepItem";
import { NodeData } from "@/stores/useWorkflow";

type NodeType = {
  id: string;
  data: NodeData;
  position: {
    x: number;
    y: number;
  };
  type: string;
  parentId?: string;
  width?: number;
  height?: number;
  sourcePosition?: Position;
  targetPosition?: Position;
  dragHandle?: string;
};

type Props = NodeProps<NodeType>;

export function StepNode({ data, isConnectable }: Props) {
  return (
    <div key={data.id}>
      <NodeContainer>
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
