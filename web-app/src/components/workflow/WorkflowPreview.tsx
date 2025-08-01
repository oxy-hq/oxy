import React, { Suspense, useEffect, useMemo } from "react";
import { ReactFlowProvider } from "@xyflow/react";
import useWorkflow from "@/stores/useWorkflow";
import { useMutation } from "@tanstack/react-query";
import useWorkflowConfig from "@/hooks/api/workflows/useWorkflowConfig";
import useWorkflowLogs from "@/hooks/api/workflows/useWorkflowLogs";
import WorkflowOutput from "./output";
import throttle from "lodash/throttle";
import { ResizableHandle } from "@/components/ui/shadcn/resizable";
import { WorkflowService } from "@/services/api";
import {
  ResizablePanel,
  ResizablePanelGroup,
} from "@/components/ui/shadcn/resizable";
import { cn } from "@/libs/shadcn/utils";
import { Button } from "@/components/ui/shadcn/button";
import {
  LoaderCircle,
  LoaderCircleIcon,
  LogsIcon,
  PlayIcon,
} from "lucide-react";
import { Skeleton } from "@/components/ui/shadcn/skeleton";
import { LogItem } from "@/services/types";

const WorkflowDiagram = React.lazy(() => import("./WorkflowDiagram"));

export interface WorkflowPreviewRef {
  run: () => void;
}

export const WorkflowPreview = ({ pathb64 }: { pathb64: string }) => {
  const path = useMemo(() => atob(pathb64), [pathb64]);
  const relativePath = path;
  const setLogs = useWorkflow((state) => state.setLogs);
  const [showOutput, setShowOutput] = React.useState(false);
  const logs = useWorkflow((state) => state.logs);

  const { data: logsData } = useWorkflowLogs(relativePath);

  useEffect(() => {
    setLogs(logsData || []);
  }, [logsData, setLogs]);

  const { data: workflowConfig } = useWorkflowConfig(path);

  const appendLogs = useWorkflow((state) => state.appendLogs);

  const run = useMutation({
    mutationFn: async ({ workflowPath }: { workflowPath: string }) => {
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

      const pathBase64 = btoa(workflowPath);
      await WorkflowService.runWorkflow(pathBase64, (logItem: LogItem) => {
        buffer.push(logItem);
        flushLogs();
      });

      // Flush any remaining logs
      if (buffer.length > 0) {
        appendLogs(buffer);
      }
    },
    onMutate: () => {
      setLogs([]);
    },
  });

  const runHandler = () => {
    setShowOutput(true);
    run.mutate({ workflowPath: relativePath });
  };

  const toggleOutput = () => {
    setShowOutput(!showOutput);
  };

  if (!workflowConfig) {
    return (
      <div className="w-full">
        <div className="flex flex-col gap-10 max-w-page-content mx-auto p-10">
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
    <ResizablePanelGroup direction="horizontal">
      <ResizablePanel
        defaultSize={50}
        minSize={20}
        className={cn(!showOutput && "flex-1!")}
      >
        <div className="relative h-full w-full">
          <ReactFlowProvider>
            <Suspense
              fallback={
                <div className="flex items-center justify-center h-full w-full">
                  <LoaderCircleIcon className="animate-spin" />
                </div>
              }
            >
              <WorkflowDiagram workflowConfig={workflowConfig} />
            </Suspense>
          </ReactFlowProvider>
          {!showOutput && (
            <Button
              variant="outline"
              className="absolute bottom-4 right-4"
              onClick={toggleOutput}
            >
              <LogsIcon className="w-4 h-4" />
            </Button>
          )}

          <Button
            variant="default"
            className="absolute top-4 right-4"
            onClick={runHandler}
            disabled={run.isPending}
          >
            {run.isPending ? (
              <LoaderCircle className="animate-spin" />
            ) : (
              <PlayIcon className="w-4 h-4" />
            )}
          </Button>
        </div>
      </ResizablePanel>
      <ResizableHandle />
      <ResizablePanel
        defaultSize={50}
        minSize={20}
        className={cn(!showOutput && "flex-[unset]!")}
      >
        {showOutput && (
          <WorkflowOutput
            logs={logs}
            showOutput={showOutput}
            toggleOutput={toggleOutput}
            isPending={run.isPending}
          />
        )}
      </ResizablePanel>
    </ResizablePanelGroup>
  );
};
