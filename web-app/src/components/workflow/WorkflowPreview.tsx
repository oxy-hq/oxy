import React, { Suspense, useEffect, useMemo } from "react";
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
  ChevronDownIcon,
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
import { useVariables, Variables } from "./WorkflowDiagram/Variables";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "../ui/shadcn/dropdown-menu";
import { ButtonGroup } from "../ui/shadcn/button-group";

const WorkflowDiagram = React.lazy(() => import("./WorkflowDiagram"));

export const WorkflowPreview = ({
  pathb64,
  runId,
  direction = "horizontal",
}: {
  pathb64: string;
  runId?: string;
  direction?: "horizontal" | "vertical";
}) => {
  const path = useMemo(() => atob(pathb64), [pathb64]);
  const relativePath = path;
  const [showOutput, setShowOutput] = React.useState(!!runId);

  const { data: workflowConfig } = useWorkflowConfig(path);
  const run = useWorkflowRun();
  const cancelRun = useCancelWorkflowRun();
  const logs = useWorkflowLogs(path, runId || "");
  const { stream, cancel } = useStreamEvents();
  const setGroupBlocks = useBlockStore((state) => state.setGroupBlocks);
  const isProcessing = useIsProcessing(path, runId || "");
  const variablesSchema = useMemo(() => {
    return {
      type: "object",
      properties: workflowConfig?.variables,
    };
  }, [workflowConfig]);
  const { setIsOpen } = useVariables();

  const groups = useGetBlocks(
    path,
    runId ? +runId : undefined,
    !!runId && !isProcessing,
  ).data;
  useEffect(() => {
    const streamCall = async (relativePath: string, runId: string) => {
      return stream
        .mutateAsync({
          workflowId: relativePath,
          runIndex: parseInt(runId, 10),
        })
        .catch((error) => {
          console.error("Error streaming events:", error);
        });
    };

    if (relativePath && runId) {
      streamCall(relativePath, runId);
      return () => {
        cancel();
      };
    }
  }, [runId, relativePath]);

  useEffect(() => {
    const firstGroup = groups && groups[0];
    if (
      firstGroup &&
      firstGroup.source_id == relativePath &&
      firstGroup.run_index.toString() === runId
    ) {
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
  }, [groups, relativePath, runId, setGroupBlocks]);

  const runHandler = async () => {
    // check config has variables
    if (!workflowConfig) return;
    const hasVariables =
      workflowConfig.variables &&
      Object.values(workflowConfig.variables).some((v) => v !== undefined);

    if (hasVariables) {
      setIsOpen(true, (data) => {
        return run.mutateAsync({
          workflowId: relativePath,
          retryType: {
            type: "no_retry",
            variables: data,
          },
        });
      });
      return;
    }
    await run.mutateAsync({
      workflowId: relativePath,
      retryType: {
        type: "no_retry",
      },
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
        retryType: {
          type: "retry",
          run_index: parseInt(runId, 10),
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
    <ResizablePanelGroup direction={direction}>
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
              <WorkflowDiagram
                workflowId={path}
                workflowConfig={workflowConfig}
                runId={runId}
              />
            </Suspense>
          </ReactFlowProvider>
          <div className="absolute bottom-4 right-4 flex items-center gap-2">
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
                <ButtonGroup>
                  <Button
                    variant="outline"
                    onClick={replayAllHandler}
                    disabled={run.isPending}
                    tooltip={"Replay Workflow Run"}
                  >
                    <RotateCcw className="w-4 h-4" />
                    Replay
                  </Button>
                  {workflowConfig.variables ? (
                    <DropdownMenu>
                      <DropdownMenuTrigger asChild>
                        <Button variant="outline" className="!pl-2">
                          <ChevronDownIcon />
                        </Button>
                      </DropdownMenuTrigger>
                      <DropdownMenuContent
                        align="end"
                        className="[--radius:1rem]"
                      >
                        <DropdownMenuItem
                          onClick={() => {
                            setIsOpen(true, (data) => {
                              return run.mutateAsync({
                                workflowId: relativePath,
                                retryType: {
                                  type: "retry_with_variables",
                                  run_index: parseInt(runId, 10),
                                  replay_id: "",
                                  variables: data,
                                },
                              });
                            });
                          }}
                        >
                          Replay With Variables
                        </DropdownMenuItem>
                      </DropdownMenuContent>
                    </DropdownMenu>
                  ) : null}
                </ButtonGroup>
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
            {workflowConfig.variables ? (
              <Variables schema={variablesSchema} />
            ) : null}
          </div>
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
            workflowId={path}
            logs={logs}
            toggleOutput={toggleOutput}
            isPending={isProcessing}
            runId={runId}
          />
        )}
      </ResizablePanel>
    </ResizablePanelGroup>
  );
};
