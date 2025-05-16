import {
  Background,
  BackgroundVariant,
  ColorMode,
  Controls,
  ReactFlow,
} from "@xyflow/react";
import { NodeType, TaskConfigWithId } from "@/stores/useWorkflow";
import { useWorkflowLayout } from "./layout/useWorkflowLayout";
import { DiagramNode } from "./DiagramNode";
import useTheme from "@/stores/useTheme";

const nodeTypes: Record<NodeType, typeof DiagramNode> = {
  execute_sql: DiagramNode,
  loop_sequential: DiagramNode,
  formatter: DiagramNode,
  agent: DiagramNode,
  workflow: DiagramNode,
  conditional: DiagramNode,
  "conditional-else": DiagramNode,
  "conditional-if": DiagramNode,
} as const;

interface WorkflowDiagramProps {
  tasks: TaskConfigWithId[];
}

const WorkflowDiagram: React.FC<WorkflowDiagramProps> = ({ tasks }) => {
  const {
    reactFlowNodes,
    reactFlowEdges,
    fitViewOptions,
    onNodesChange,
    onEdgesChange,
  } = useWorkflowLayout(tasks);

  const { theme } = useTheme();

  return (
    <div className="w-full h-full">
      <ReactFlow
        colorMode={theme as ColorMode}
        nodeTypes={nodeTypes}
        proOptions={{ hideAttribution: true }}
        onNodesChange={onNodesChange}
        onEdgesChange={onEdgesChange}
        nodes={reactFlowNodes}
        edges={reactFlowEdges}
        fitView
        draggable={false}
        nodesDraggable={false}
      >
        <Controls showInteractive={false} fitViewOptions={fitViewOptions} />
        <Background color="#ccc" variant={BackgroundVariant.Dots} />
      </ReactFlow>
    </div>
  );
};

export default WorkflowDiagram;
