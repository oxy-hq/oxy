import React, { useEffect, useMemo } from "react";

import { v4 as uuidv4 } from "uuid";

import { useParams } from "react-router-dom";

import { ReactFlowProvider } from "@xyflow/react";
import useWorkflow, {
  TaskConfig,
  TaskType,
  TaskConfigWithId,
} from "@/stores/useWorkflow";
import { useMutation } from "@tanstack/react-query";
import runWorkflow, { LogItem } from "@/hooks/api/runWorkflow";
import useWorkflowConfig from "@/hooks/api/useWorkflowConfig.ts";
import useWorkflowLogs from "@/hooks/api/useWorkflowLogs";

import WorkflowDiagram from "./WorkflowDiagram";
import WorkflowPageHeader from "./WorkflowPageHeader";
import WorkflowOutput from "./WorkflowOutput";
import { throttle } from "lodash";

const addTaskId = (tasks: TaskConfig[]): TaskConfigWithId[] => {
  return tasks.map((task) => {
    if (task.type === TaskType.LOOP_SEQUENTIAL) {
      return {
        ...task,
        type: TaskType.LOOP_SEQUENTIAL,
        tasks: addTaskId(task.tasks),
        id: uuidv4(),
      };
    }
    return { ...task, id: uuidv4() } as TaskConfigWithId;
  });
};

const WorkflowPage: React.FC = () => {
  const pathb64 = useParams<{ pathb64: string }>().pathb64!;
  const path = useMemo(() => atob(pathb64), [pathb64]);
  const relativePath = path;
  const workflow = useWorkflow((state) => state.workflow);
  const setLogs = useWorkflow((state) => state.setLogs);
  const [showOutput, setShowOutput] = React.useState(false);
  const logs = useWorkflow((state) => state.logs);
  const setWorkflow = useWorkflow((state) => state.setWorkflow);

  const { data: logsData } = useWorkflowLogs(relativePath);

  useEffect(() => {
    setLogs(logsData || []);
  }, [logsData, setLogs]);

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
  const appendLogs = useWorkflow((state) => state.appendLogs);

  const run = useMutation({
    mutationFn: runWorkflow,
    onMutate: () => {
      setLogs([]);
    },
    onSuccess: async (data) => {
      if (!data) return;
      let buffer: LogItem[] = [];
      const flushLogs = throttle(
        () => {
          const logsToAppend = [...buffer];
          appendLogs(logsToAppend);
          buffer = [];
        },
        500,
        { leading: true, trailing: true },
      );

      for await (const logItem of data) {
        buffer.push(logItem);
        flushLogs();
      }
    },
  });
  const handleRun = async () => {
    setShowOutput(true);
    run.mutate({ workflowPath: relativePath });
  };

  const toggleOutput = () => {
    setShowOutput(!showOutput);
  };

  if (workflow === null) {
    return <div>Loading...</div>;
  }

  return (
    <div className="w-full h-full flex flex-col">
      <WorkflowPageHeader
        path={path}
        onRun={handleRun}
        isRunning={run.isPending}
      />
      <div className="flex h-full flex-1 flex-col w-full">
        <div className="flex-1 h-full w-full">
          <ReactFlowProvider>
            <WorkflowDiagram tasks={workflow.tasks} />
          </ReactFlowProvider>
        </div>
      </div>
      <WorkflowOutput
        logs={logs}
        showOutput={showOutput}
        toggleOutput={toggleOutput}
        isPending={run.isPending}
      />
    </div>
  );
};

export type StepData = {
  id: string;
  name: string;
  type: string;
};

export default WorkflowPage;
