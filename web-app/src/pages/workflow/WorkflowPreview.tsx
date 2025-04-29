import React, { useEffect, useMemo } from "react";

import { v4 as uuidv4 } from "uuid";

import { ReactFlowProvider } from "@xyflow/react";
import useWorkflow, {
  TaskConfig,
  TaskType,
  TaskConfigWithId,
} from "@/stores/useWorkflow";
import useWorkflowConfig from "@/hooks/api/useWorkflowConfig.ts";

import WorkflowDiagram from "./WorkflowDiagram";

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

const WorkflowPreview: React.FC<{ pathb64: string }> = ({ pathb64 }) => {
  const path = useMemo(() => atob(pathb64), [pathb64]);
  const workflow = useWorkflow((state) => state.workflow);
  const setWorkflow = useWorkflow((state) => state.setWorkflow);
  const setSelectedNodeId = useWorkflow((state) => state.setSelectedNodeId);

  useEffect(() => {
    setSelectedNodeId(null);
  }, [setSelectedNodeId]);

  const { data: workflowConfig } = useWorkflowConfig(path);
  useEffect(() => {
    if (workflowConfig) {
      const tasks = addTaskId(workflowConfig.tasks);
      const workflow = { ...workflowConfig, tasks, path };
      setWorkflow(workflow);
      setSelectedNodeId(null);
    }
  }, [workflowConfig, setWorkflow, path, setSelectedNodeId]);

  if (workflow === null) {
    return <div>Loading...</div>;
  }

  return (
    <ReactFlowProvider>
      <WorkflowDiagram tasks={workflow.tasks} />
    </ReactFlowProvider>
  );
};

export default WorkflowPreview;
