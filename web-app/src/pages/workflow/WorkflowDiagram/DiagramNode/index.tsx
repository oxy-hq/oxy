import { Handle, NodeProps, Position } from "@xyflow/react";

import { NodeContent } from "./NodeContent";
import { NodeData, NodeType } from "@/stores/useWorkflow";

type Node = {
  id: string;
  data: NodeData;
  position: {
    x: number;
    y: number;
  };
  type: NodeType;
  parentId?: string;
  width?: number;
  height?: number;
  sourcePosition?: Position;
  targetPosition?: Position;
  dragHandle?: string;
};

type Props = NodeProps<Node>;

export function DiagramNode({
  data,
  isConnectable,
  type,
  width,
  height,
}: Props) {
  return (
    <div key={data.id}>
      <Handle
        type="target"
        position={Position.Top}
        isConnectable={isConnectable}
        className="invisible !top-0.5"
      />
      <NodeContent
        task={data.task}
        data={data}
        type={type}
        width={width}
        height={height}
      />
      <Handle
        type="source"
        position={Position.Bottom}
        isConnectable={isConnectable}
        className="invisible !bottom-0.5"
      />
    </div>
  );
}
