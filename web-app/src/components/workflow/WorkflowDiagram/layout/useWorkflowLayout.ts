import useWorkflow, {
  TaskConfig,
  TaskConfigWithId,
  TaskType,
} from "@/stores/useWorkflow";
import { useEffect, useMemo } from "react";
import type { FitViewOptions } from "@xyflow/react";
import { calculateNodesSize, getLayoutedElements } from ".";

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

export const useWorkflowLayout = (
  workflowId: string,
  tasks: TaskConfig[],
  runId?: string,
) => {
  const baseNodes = useWorkflow((state) => state.baseNodes);
  const edges = useWorkflow((state) => state.edges);
  const nodes = useWorkflow((state) => state.nodes);
  const setNodes = useWorkflow((state) => state.setNodes);
  const initFromTasks = useWorkflow((state) => state.initFromTasks);
  const tasksWithId = useMemo(() => {
    return addTaskId(workflowId, tasks, runId);
  }, [workflowId, tasks, runId]);
  const fitViewOptions: FitViewOptions = useMemo(() => {
    return {
      maxZoom: 1,
      minZoom: 0.1,
      nodes,
      duration: 0,
    };
  }, [nodes]);

  useEffect(() => {
    initFromTasks(tasksWithId);
  }, [tasksWithId, initFromTasks]);

  useEffect(() => {
    const updateLayout = async () => {
      const nodesWithSize = calculateNodesSize(baseNodes);
      const newNodes = await getLayoutedElements(nodesWithSize, edges);
      setNodes(newNodes);
    };
    updateLayout();
  }, [baseNodes, edges, setNodes]);

  return {
    fitViewOptions,
    nodes,
    edges,
  };
};
