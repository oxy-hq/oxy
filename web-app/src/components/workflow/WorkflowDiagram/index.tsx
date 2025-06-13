import {
  Background,
  BackgroundVariant,
  ColorMode,
  Controls,
  ReactFlow,
} from "@xyflow/react";
import useWorkflow, {
  NodeType,
  TaskConfig,
  TaskConfigWithId,
  TaskType,
} from "@/stores/useWorkflow";
import { useWorkflowLayout } from "./layout/useWorkflowLayout";
import { DiagramNode } from "./DiagramNode";
import useTheme from "@/stores/useTheme";
import React, { useEffect } from "react";
import { v4 as uuidv4 } from "uuid";
import { Skeleton } from "@/components/ui/shadcn/skeleton";
import { WorkflowConfig } from "@/stores/useWorkflow";

const getTaskId = (task_name: string) => {
  return task_name + "__" + uuidv4();
};

const addTaskId = (tasks: TaskConfig[]): TaskConfigWithId[] => {
  return tasks.map((task) => {
    if (task.type === TaskType.LOOP_SEQUENTIAL) {
      return {
        ...task,
        type: TaskType.LOOP_SEQUENTIAL,
        tasks: addTaskId(task.tasks),
        id: getTaskId(task.name),
      };
    }
    if (task.type === TaskType.CONDITIONAL) {
      return {
        ...task,
        conditions: task.conditions.map((c) => ({
          ...c,
          tasks: addTaskId(c.tasks),
        })),
        type: TaskType.CONDITIONAL,
        else: task.else ? addTaskId(task.else) : undefined,
        id: getTaskId(task.name),
      };
    }
    return { ...task, id: getTaskId(task.name) } as TaskConfigWithId;
  });
};

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
  workflowConfig: WorkflowConfig;
}

const WorkflowDiagram: React.FC<WorkflowDiagramProps> = ({
  workflowConfig,
}) => {
  const workflow = useWorkflow((state) => state.workflow);
  const setWorkflow = useWorkflow((state) => state.setWorkflow);

  const setSelectedNodeId = useWorkflow((state) => state.setSelectedNodeId);

  useEffect(() => {
    if (workflowConfig) {
      const tasks = addTaskId(workflowConfig.tasks);
      const workflow = {
        ...workflowConfig,
        tasks,
        path: workflowConfig.path ?? "",
      };
      setWorkflow(workflow);
      setSelectedNodeId(null);
    }
  }, [workflowConfig, setWorkflow, setSelectedNodeId]);

  const {
    reactFlowNodes,
    reactFlowEdges,
    fitViewOptions,
    onNodesChange,
    onEdgesChange,
  } = useWorkflowLayout(workflow?.tasks ?? []);

  const { theme } = useTheme();

  if (workflow === null) {
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
