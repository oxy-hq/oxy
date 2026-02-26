import type { ReactFlowInstance } from "@xyflow/react";
import { Background, BackgroundVariant, type ColorMode, Controls, ReactFlow } from "@xyflow/react";
import type React from "react";
import { useRef } from "react";
import { Skeleton } from "@/components/ui/shadcn/skeleton";
import useTheme from "@/stores/useTheme";
import useWorkflow, { type NodeType, type WorkflowConfig } from "@/stores/useWorkflow";
import { DiagramNode } from "./DiagramNode";
import { usePersistedViewport } from "./hooks/usePersistedViewport";
import { useWorkflowLayout } from "./layout/useWorkflowLayout";
import { restoreOrFit } from "./utils/viewport";

const nodeTypes: Record<NodeType, typeof DiagramNode> = {
  execute_sql: DiagramNode,
  semantic_query: DiagramNode,
  loop_sequential: DiagramNode,
  formatter: DiagramNode,
  agent: DiagramNode,
  workflow: DiagramNode,
  conditional: DiagramNode,
  "conditional-else": DiagramNode,
  "conditional-if": DiagramNode,
  omni_query: DiagramNode,
  visualize: DiagramNode
} as const;

interface WorkflowDiagramProps {
  workflowId: string;
  runId?: string;
  workflowConfig: WorkflowConfig;
}

const WorkflowDiagram: React.FC<WorkflowDiagramProps> = ({ workflowId, runId, workflowConfig }) => {
  const onNodesChange = useWorkflow((state) => state.onNodesChange);
  const onEdgesChange = useWorkflow((state) => state.onEdgesChange);
  const { nodes, edges, fitViewOptions } = useWorkflowLayout(
    workflowId,
    workflowConfig.tasks,
    runId
  );

  const reactFlowRef = useRef<ReactFlowInstance | null>(null);
  const { load: loadSavedViewport, save: saveViewport } = usePersistedViewport(
    `oxy.workflow.viewport.${workflowId}`
  );

  const { theme } = useTheme();

  if (nodes.length === 0) {
    return (
      <div className='w-full'>
        <div className='mx-auto flex max-w-[742px] flex-col gap-10 py-10'>
          {Array.from({ length: 3 }).map((_, index) => (
            <div key={index} className='flex flex-col gap-4'>
              <Skeleton className='h-4 max-w-[200px]' />
              <Skeleton className='h-4 max-w-[500px]' />
              <Skeleton className='h-4 max-w-[500px]' />
            </div>
          ))}
        </div>
      </div>
    );
  }

  return (
    <div className='h-full w-full'>
      <ReactFlow
        key={workflowId}
        onInit={(instance) => {
          reactFlowRef.current = instance as unknown as ReactFlowInstance;
          const saved = loadSavedViewport();
          restoreOrFit(instance as unknown as ReactFlowInstance, saved, fitViewOptions);
        }}
        onMoveEnd={(..._args: unknown[]) => {
          const viewport = _args[1] as { x: number; y: number; zoom: number } | undefined;
          saveViewport(viewport);
        }}
        colorMode={theme as ColorMode}
        nodeTypes={nodeTypes}
        proOptions={{ hideAttribution: true }}
        onNodesChange={onNodesChange}
        onEdgesChange={onEdgesChange}
        nodes={nodes}
        edges={edges}
        draggable={false}
        nodesDraggable={false}
      >
        <Controls showInteractive={false} />
        <Background
          color={theme === "dark" ? "#2a2a2a" : "#ddd"}
          variant={BackgroundVariant.Dots}
        />
      </ReactFlow>
    </div>
  );
};

export default WorkflowDiagram;
