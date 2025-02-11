import { Handle, NodeProps, Position } from "@xyflow/react";

import { StepData } from ".";
import { NodeContainer } from "./NodeContainer";
import { StepItem } from "./StepItem";

export type NodeData = {
  step: StepData;
  id: string;
};

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
  // return <div key={data.id} style={{
  //   border: "1px solid #000",
  //   // backgroundColor: "#fff",
  //   width: "100%",
  //   height: "100%"
  // }}></div>
  return (
    <div key={data.id}>
      <NodeContainer>
        <Handle
          type='target'
          position={Position.Top}
          isConnectable={isConnectable}
          style={{
            visibility: "hidden",
            top: "3px"
          }}
        />
        <StepItem step={data.step} />
        <Handle
          type='source'
          position={Position.Bottom}
          isConnectable={isConnectable}
          style={{
            visibility: "hidden",
            bottom: "1px"
          }}
        />
      </NodeContainer>
    </div>
  );
}
