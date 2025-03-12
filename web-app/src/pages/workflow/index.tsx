import { useEffect, useMemo, useRef } from "react";

import { v4 as uuidv4 } from "uuid";

import { useParams } from "react-router-dom";

import WorkflowDiagram from "./WorkflowDiagram";
import useProjectPath from "@/stores/useProjectPath";
import { ReactFlowProvider } from "@xyflow/react";
import useWorkflow, {
  TaskConfig,
  TaskType,
  TaskConfigWithId,
} from "@/stores/useWorkflow";
import RightSidebar from "./RightSidebar";
import { useMutation } from "@tanstack/react-query";
import runWorkflow from "@/hooks/api/runWorkflow";
import React from "react";
import WorkflowPageHeader from "./WorkflowPageHeader";
import WorkflowOutput from "./WorkflowOutput";
import useWorkflowConfig from "@/hooks/api/useWorkflowConfig.ts";
import useWorkflowLogs from "@/hooks/api/useWorkflowLogs";

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
  const projectPath = useProjectPath((state) => state.projectPath);
  const relativePath = path.replace(projectPath, "").replace(/^\//, "");
  const workflow = useWorkflow((state) => state.workflow);
  const setLogs = useWorkflow((state) => state.setLogs);
  const [showOutput, setShowOutput] = React.useState(false);
  const logs = useWorkflow((state) => state.logs);
  const setWorkflow = useWorkflow((state) => state.setWorkflow);
  const outputEnd = useRef<HTMLDivElement | null>(null);
  const scrollToBottom = () => {
    outputEnd.current?.scrollIntoView({ behavior: "smooth" });
  };

  const { data: logsData } = useWorkflowLogs(relativePath);

  useEffect(() => {
    setLogs(logsData || []);
  }, [logsData, setLogs]);

  useEffect(() => {
    scrollToBottom();
  }, [logs]);
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
  const appendLog = useWorkflow((state) => state.appendLog);
  const run = useMutation({
    mutationFn: runWorkflow,
    onMutate: () => {
      setLogs([]);
    },
    onSuccess: async (data) => {
      if (!data) return;
      for await (const logItem of data) {
        appendLog(logItem);
      }
    },
  });
  const handleRun = async () => {
    setShowOutput(true);
    run.mutate({ projectPath, workflowPath: relativePath });
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
        <RightSidebar key={pathb64} />
      </div>
      <WorkflowOutput
        logs={logs}
        showOutput={showOutput}
        toggleOutput={toggleOutput}
        isPending={run.isPending}
        outputEnd={outputEnd}
      />
    </div>
  );
};

export default WorkflowPage;
