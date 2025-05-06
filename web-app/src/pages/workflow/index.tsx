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
import WorkflowOutput from "./output";
import { throttle } from "lodash";
import { ResizableHandle } from "@/components/ui/shadcn/resizable";
import {
  ResizablePanel,
  ResizablePanelGroup,
} from "@/components/ui/shadcn/resizable";
import { cn } from "@/libs/shadcn/utils";

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

const Workflow: React.FC<{ pathb64: string }> = ({ pathb64 }) => {
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
      <ResizablePanelGroup direction="horizontal">
        <ResizablePanel
          defaultSize={50}
          minSize={20}
          className={cn(!showOutput && "flex-1!")}
        >
          <ReactFlowProvider>
            <WorkflowDiagram tasks={workflow.tasks} />
          </ReactFlowProvider>
        </ResizablePanel>
        <ResizableHandle />
        <ResizablePanel
          defaultSize={50}
          minSize={20}
          className={cn(!showOutput && "flex-[unset]!")}
        >
          <WorkflowOutput
            logs={logs}
            showOutput={showOutput}
            toggleOutput={toggleOutput}
            isPending={run.isPending}
          />
        </ResizablePanel>
      </ResizablePanelGroup>
    </div>
  );
};

export type StepData = {
  id: string;
  name: string;
  type: string;
};

const WorkflowPage = () => {
  const { pathb64 } = useParams();
  return <Workflow key={pathb64 ?? ""} pathb64={pathb64 ?? ""} />;
};

export default WorkflowPage;
