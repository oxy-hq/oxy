import React, { Suspense, useEffect, useMemo, useRef } from "react";
import { ReactFlowProvider } from "@xyflow/react";
import useWorkflowConfig from "@/hooks/api/workflows/useWorkflowConfig";
import WorkflowOutput from "./output";
import { ResizableHandle } from "@/components/ui/shadcn/resizable";
import {
  ResizablePanel,
  ResizablePanelGroup,
} from "@/components/ui/shadcn/resizable";
import { cn } from "@/libs/shadcn/utils";
import { Button } from "@/components/ui/shadcn/button";
import {
  History,
  LoaderCircle,
  LoaderCircleIcon,
  LogsIcon,
  PlayIcon,
  RotateCcw,
  StopCircle,
} from "lucide-react";
import { Skeleton } from "@/components/ui/shadcn/skeleton";
import {
  useCancelWorkflowRun,
  useGetBlocks,
  useIsProcessing,
  useStreamEvents,
  useWorkflowLogs,
  useWorkflowRun,
} from "./useWorkflowRun";
import { useBlockStore } from "@/stores/block";
import { WorkflowRuns } from "./WorkflowRuns";

const WorkflowDiagram = React.lazy(() => import("./WorkflowDiagram"));

export const WorkflowPreview = ({
  pathb64,
  runId,
}: {
  pathb64: string;
  runId?: string;
}) => {
  const path = useMemo(() => atob(pathb64), [pathb64]);
  const relativePath = path;
  const [showOutput, setShowOutput] = React.useState(!!runId);
  const [showRuns, setShowRuns] = React.useState(!runId);
  const { data: workflowConfig } = useWorkflowConfig(path);
  const run = useWorkflowRun();
  const cancelRun = useCancelWorkflowRun();
  const logs = useWorkflowLogs(path, runId || "");
  const { stream, cancel } = useStreamEvents();
  const setGroupBlocks = useBlockStore((state) => state.setGroupBlocks);
  const [isStreamFinished, setIsStreamFinished] = React.useState(false);
  const isStreamingCalled = useRef(false);
  const isProcessing = useIsProcessing(path, runId || "");
  const groups = useGetBlocks(
    path,
    runId ? +runId : undefined,
    !!runId && isStreamFinished,
  ).data;
  useEffect(() => {
    const streamCall = async (runId: string) => {
      return stream
        .mutateAsync({
          workflowId: relativePath,
          runIndex: parseInt(runId, 10),
        })
        .catch((error) => {
          console.error("Error streaming events:", error);
        })
        .finally(() => {
          setIsStreamFinished(true);
        });
    };

    if (runId && !isStreamingCalled.current) {
      isStreamingCalled.current = true;
      streamCall(runId);
    }
  }, []);

  useEffect(() => {
    return () => {
      cancel();
    };
  }, []);

  useEffect(() => {
    if (groups) {
      groups.forEach((group) => {
        setGroupBlocks(
          group,
          group.blocks,
          group.children,
          group.error,
          group.metadata,
        );
      });
    }
  }, [groups]);

  const runHandler = async () => {
    await run.mutateAsync({
      workflowId: relativePath,
    });
  };

  const cancelRunHandler = async () => {
    if (runId) {
      await cancelRun.mutateAsync({
        sourceId: relativePath,
        runIndex: parseInt(runId, 10),
      });
    }
  };

  const replayAllHandler = async () => {
    if (runId) {
      await run.mutateAsync({
        workflowId: relativePath,
        retryParam: {
          run_id: parseInt(runId, 10),
          replay_id: "",
        },
      });
      setShowOutput(true);
      await stream.mutateAsync({
        workflowId: relativePath,
        runIndex: parseInt(runId, 10),
      });
    }
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
        <ResizablePanelGroup direction="vertical">
          <ResizablePanel defaultSize={70} minSize={20}>
            <div className="relative h-full w-full">
              <ReactFlowProvider>
                <Suspense
                  fallback={
                    <div className="flex items-center justify-center h-full w-full">
                      <LoaderCircleIcon className="animate-spin" />
                    </div>
                  }
                >
                  <WorkflowDiagram
                    workflowId={path}
                    workflowConfig={workflowConfig}
                    runId={runId}
                  />
                </Suspense>
              </ReactFlowProvider>
              <div className="absolute bottom-4 right-4 flex items-center gap-2">
                {!showRuns && (
                  <Button
                    tooltip={"Show Workflow Runs"}
                    variant="outline"
                    onClick={() => setShowRuns(!showRuns)}
                  >
                    <History className="w-4 h-4" />
                  </Button>
                )}
                {!showOutput && (
                  <Button
                    variant="outline"
                    onClick={toggleOutput}
                    tooltip={"Show Logs Output"}
                  >
                    <LogsIcon className="w-4 h-4" />
                  </Button>
                )}
              </div>

              <div className="absolute top-4 right-4 flex items-center gap-2">
                {!!runId &&
                  (isProcessing ? (
                    <Button
                      variant="outline"
                      onClick={cancelRunHandler}
                      disabled={cancelRun.isPending}
                      tooltip={"Cancel Workflow Run"}
                    >
                      <StopCircle className="w-4 h-4" />
                    </Button>
                  ) : (
                    <Button
                      variant="outline"
                      onClick={replayAllHandler}
                      disabled={run.isPending}
                      tooltip={"Replay Workflow Run"}
                    >
                      <RotateCcw className="w-4 h-4" />
                    </Button>
                  ))}
                <Button
                  variant="default"
                  onClick={runHandler}
                  disabled={run.isPending}
                  tooltip={run.isPending ? "Running..." : "Run Workflow"}
                >
                  {run.isPending ? (
                    <LoaderCircle className="animate-spin" />
                  ) : (
                    <PlayIcon className="w-4 h-4" />
                  )}
                </Button>
              </div>
            </div>
          </ResizablePanel>
          <ResizableHandle />

          <ResizablePanel
            defaultSize={30}
            minSize={20}
            className={cn(!showRuns && "flex-[unset]!")}
          >
            {showRuns && (
              <WorkflowRuns
                workflowId={path}
                onClose={() => setShowRuns(false)}
              />
            )}
          </ResizablePanel>
        </ResizablePanelGroup>
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
