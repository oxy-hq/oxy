import { Handle, NodeProps, Position } from "@xyflow/react";

import { TaskData } from ".";
import { NodeContainer } from "./NodeContainer";
import { TaskItem } from "./TaskItem";

export type NodeData = {
  task: TaskData;
  id: string;
};

type Props = NodeProps & {
  data: {
    task: TaskData;
    id: string;
  };
};

export function TaskNode({ data, isConnectable }: Props) {
  return (
    <div key={data.id}>
      <NodeContainer>
        <Handle
          type="target"
          position={Position.Top}
          isConnectable={isConnectable}
          style={{
            visibility: "hidden",
            top: "3px",
          }}
        />
        <TaskItem task={data.task} />
        <Handle
          type="source"
          position={Position.Bottom}
          isConnectable={isConnectable}
          style={{
            visibility: "hidden",
            bottom: "1px",
          }}
        />
      </NodeContainer>
    </div>
  );
}
