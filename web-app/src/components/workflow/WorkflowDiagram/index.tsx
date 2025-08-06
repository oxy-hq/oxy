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
import { Skeleton } from "@/components/ui/shadcn/skeleton";
import { WorkflowConfig } from "@/stores/useWorkflow";

const addTaskId = (
  workflowId: string,
  tasks: TaskConfig[],
  runId?: string,
  parentId?: string,
  subWorkflowTaskId?: string,
): TaskConfigWithId[] => {
  return tasks.map((task) => {
    const taskId = parentId ? `${parentId}.${task.name}` : task.name;
    if (task.type === TaskType.LOOP_SEQUENTIAL) {
      return {
        ...task,
        type: TaskType.LOOP_SEQUENTIAL,
        tasks: addTaskId(
          workflowId,
          task.tasks,
          runId,
          taskId,
          subWorkflowTaskId,
        ),
        id: taskId,
        workflowId,
        subWorkflowTaskId,
        runId,
      };
    }
    if (task.type === TaskType.WORKFLOW) {
      return {
        ...task,
        type: TaskType.WORKFLOW,
        tasks: addTaskId(task.src, task.tasks ?? [], runId, taskId, taskId),
        id: taskId,
        workflowId,
        runId,
        subWorkflowTaskId,
      };
    }
    if (task.type === TaskType.CONDITIONAL) {
      return {
        ...task,
        conditions: task.conditions.map((c) => ({
          ...c,
          tasks: addTaskId(
            workflowId,
            c.tasks,
            runId,
            taskId,
            subWorkflowTaskId,
          ),
        })),
        type: TaskType.CONDITIONAL,
        else: task.else
          ? addTaskId(workflowId, task.else, runId, taskId)
          : undefined,
        id: taskId,
        workflowId,
        runId,
        subWorkflowTaskId,
      };
    }
    return {
      ...task,
      id: taskId,
      workflowId,
      runId,
      subWorkflowTaskId,
    } as TaskConfigWithId;
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
  workflowId?: string;
  runId?: string;
  workflowConfig: WorkflowConfig;
}

const WorkflowDiagram: React.FC<WorkflowDiagramProps> = ({
  workflowId,
  runId,
  workflowConfig,
}) => {
  const workflow = useWorkflow((state) => state.workflow);
  const setWorkflow = useWorkflow((state) => state.setWorkflow);

  useEffect(() => {
    if (workflowId && workflowConfig) {
      const tasks = addTaskId(workflowId, workflowConfig.tasks, runId);
      const workflow = {
        ...workflowConfig,
        tasks,
        id: workflowId,
        path: workflowConfig.path ?? "",
      };
      setWorkflow(workflow);
    }
  }, [workflowId, workflowConfig, setWorkflow, runId]);

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
