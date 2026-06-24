import type { FitViewOptions } from "@xyflow/react";
import { useEffect, useMemo } from "react";
import useAutomation, {
  type TaskConfig,
  type TaskConfigWithId,
  TaskType
} from "@/stores/useAutomation";
import { calculateNodesSize, getLayoutedElements } from ".";

const addTaskId = (
  automationId: string,
  tasks: TaskConfig[],
  runId?: string,
  parentId?: string,
  subAutomationTaskId?: string
): TaskConfigWithId[] => {
  return tasks.map((task) => {
    const taskId = parentId ? `${parentId}.${task.name}` : task.name;
    if (task.type === TaskType.LOOP_SEQUENTIAL) {
      return {
        ...task,
        type: TaskType.LOOP_SEQUENTIAL,
        tasks: addTaskId(automationId, task.tasks, runId, taskId, subAutomationTaskId),
        id: taskId,
        automationId,
        subAutomationTaskId,
        runId
      };
    }
    if (task.type === TaskType.WORKFLOW) {
      return {
        ...task,
        type: TaskType.WORKFLOW,
        tasks: addTaskId(task.src, task.tasks ?? [], runId, taskId, taskId),
        id: taskId,
        automationId,
        runId,
        subAutomationTaskId
      };
    }
    if (task.type === TaskType.CONDITIONAL) {
      return {
        ...task,
        conditions: task.conditions.map((c) => ({
          ...c,
          tasks: addTaskId(automationId, c.tasks, runId, taskId, subAutomationTaskId)
        })),
        type: TaskType.CONDITIONAL,
        else: task.else ? addTaskId(automationId, task.else, runId, taskId) : undefined,
        id: taskId,
        automationId,
        runId,
        subAutomationTaskId
      };
    }
    return {
      ...task,
      id: taskId,
      automationId,
      runId,
      subAutomationTaskId
    } as TaskConfigWithId;
  });
};

export const useAutomationLayout = (automationId: string, tasks: TaskConfig[], runId?: string) => {
  const baseNodes = useAutomation((state) => state.baseNodes);
  const edges = useAutomation((state) => state.edges);
  const nodes = useAutomation((state) => state.nodes);
  const setNodes = useAutomation((state) => state.setNodes);
  const initFromTasks = useAutomation((state) => state.initFromTasks);
  const tasksWithId = useMemo(() => {
    return addTaskId(automationId, tasks, runId);
  }, [automationId, tasks, runId]);
  const fitViewOptions: FitViewOptions = useMemo(() => {
    return {
      maxZoom: 1,
      minZoom: 0.1,
      nodes,
      duration: 0
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
    edges
  };
};
