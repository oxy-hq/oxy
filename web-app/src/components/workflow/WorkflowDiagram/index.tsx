import {
  Background,
  BackgroundVariant,
  ColorMode,
  Controls,
  ReactFlow,
} from "@xyflow/react";
import useWorkflow, { NodeType } from "@/stores/useWorkflow";
import { DiagramNode } from "./DiagramNode";
import useTheme from "@/stores/useTheme";
import React from "react";
import { Skeleton } from "@/components/ui/shadcn/skeleton";
import { WorkflowConfig } from "@/stores/useWorkflow";
import { useWorkflowLayout } from "./layout/useWorkflowLayout";

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
  workflowId: string;
  runId?: string;
  workflowConfig: WorkflowConfig;
}

const WorkflowDiagram: React.FC<WorkflowDiagramProps> = ({
  workflowId,
  runId,
  workflowConfig,
}) => {
  const onNodesChange = useWorkflow((state) => state.onNodesChange);
  const onEdgesChange = useWorkflow((state) => state.onEdgesChange);
  const { nodes, edges, fitViewOptions } = useWorkflowLayout(
    workflowId,
    workflowConfig.tasks,
    runId,
  );

  const { theme } = useTheme();

  if (nodes.length === 0) {
    return (
      <div className="w-full">
        <div className="flex flex-col gap-10 max-w-[742px] mx-auto py-10">
          {Array.from({ length: 3 }).map((_, index) => (
            <div key={index} className="flex flex-col gap-4">
              <Skeleton className="h-4 max-w-[200px]" />
              <Skeleton className="h-4 max-w-[500px]" />
              <Skeleton className="h-4 max-w-[500px]" />
            </div>
          ))}
        </div>
      </div>
    );
  }

  return (
    <div className="w-full h-full">
      <ReactFlow
        colorMode={theme as ColorMode}
        nodeTypes={nodeTypes}
        proOptions={{ hideAttribution: true }}
        onNodesChange={onNodesChange}
        onEdgesChange={onEdgesChange}
        nodes={nodes}
        edges={edges}
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
