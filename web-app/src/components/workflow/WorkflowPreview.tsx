import { ReactFlowProvider } from "@xyflow/react";
import { get } from "lodash";
import {
  ChevronDownIcon,
  CircleAlert,
  LoaderCircle,
  LoaderCircleIcon,
  LogsIcon,
  PlayIcon,
  RotateCcw,
  StopCircle
} from "lucide-react";
import React, { Suspense, useCallback, useEffect, useMemo } from "react";
import { Button } from "@/components/ui/shadcn/button";
import {
  ResizableHandle,
  ResizablePanel,
  ResizablePanelGroup
} from "@/components/ui/shadcn/resizable";
import { Skeleton } from "@/components/ui/shadcn/skeleton";
import useWorkflowConfig from "@/hooks/api/workflows/useWorkflowConfig";
import { cn } from "@/libs/shadcn/utils";
import { useBlockStore } from "@/stores/block";
import { Alert, AlertDescription, AlertTitle } from "../ui/shadcn/alert";
import { ButtonGroup } from "../ui/shadcn/button-group";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger
} from "../ui/shadcn/dropdown-menu";
import WorkflowOutput from "./output";
import {
  useCancelWorkflowRun,
  useGetBlocks,
  useIsProcessing,
  useStreamEvents,
  useWorkflowLogs,
  useWorkflowRun
} from "./useWorkflowRun";
import { useVariables, Variables } from "./WorkflowDiagram/Variables";

const WorkflowDiagram = React.lazy(() => import("./WorkflowDiagram"));

export const WorkflowPreview = ({
  pathb64,
  runId,
  direction = "horizontal"
}: {
  pathb64: string;
  runId?: string;
  direction?: "horizontal" | "vertical";
}) => {
  const path = useMemo(() => atob(pathb64), [pathb64]);
  const relativePath = path;
  const [showOutput, setShowOutput] = React.useState(!!runId);

  // Show output panel when runId becomes available
  useEffect(() => {
    if (runId) {
      // eslint-disable-next-line react-hooks/set-state-in-effect
      setShowOutput(true);
    }
  }, [runId]);

  const { data: workflowConfig, error } = useWorkflowConfig(path);
  const run = useWorkflowRun();
  const cancelRun = useCancelWorkflowRun();
  const logs = useWorkflowLogs(path, runId || "");
  const { stream } = useStreamEvents();
  const setGroupBlocks = useBlockStore((state) => state.setGroupBlocks);
  const isProcessing = useIsProcessing(path, runId || "");
  const variablesSchema = useMemo(() => {
    return {
      type: "object",
      properties: workflowConfig?.variables
    };
  }, [workflowConfig]);
  const { setIsOpen } = useVariables();

  const groups = useGetBlocks(path, runId ? +runId : undefined, !!runId && !isProcessing).data;

  const streamCall = useCallback(
    async (relativePath: string, runId: string, abortRef: AbortController) => {
      return stream
        .mutateAsync({
          sourceId: relativePath,
          runIndex: parseInt(runId, 10),
          abortRef: abortRef.signal
        })
        .catch((error) => {
          console.error("Error streaming events:", error);
        });
    },
    [stream]
  );

  // biome-ignore lint/correctness/useExhaustiveDependencies: <explanation>
  useEffect(() => {
    const abortRef = new AbortController();
    if (relativePath && runId) {
      streamCall(relativePath, runId, abortRef);
      return () => {
        abortRef.abort();
      };
    }
  }, [runId, relativePath]);

  useEffect(() => {
    const firstGroup = groups?.[0];
    if (
      firstGroup &&
      firstGroup.source_id === relativePath &&
      firstGroup.run_index.toString() === runId
    ) {
      groups.forEach((group) => {
        setGroupBlocks(group, group.blocks, group.children, group.error, group.metadata);
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
            variables: data
          }
        });
      });
      return;
    }
    await run.mutateAsync({
      workflowId: relativePath,
      retryType: {
        type: "no_retry"
      }
    });
  };

  const cancelRunHandler = async () => {
    if (runId) {
      await cancelRun.mutateAsync({
        sourceId: relativePath,
        runIndex: parseInt(runId, 10)
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
          replay_id: ""
        }
      });
      setShowOutput(true);
      await stream.mutateAsync({
        sourceId: relativePath,
        runIndex: parseInt(runId, 10)
      });
    }
  };

  const toggleOutput = () => {
    setShowOutput(!showOutput);
  };

  if (!workflowConfig && !error) {
    return (
      <div className='w-full'>
        <div className='mx-auto flex max-w-page-content flex-col gap-10 p-10'>
          {Array.from({ length: 3 }).map((_, index) => (
            <div key={index} className='flex flex-col gap-4'>
              <Skeleton className='h-4 max-w-[200px]' />
              <Skeleton className='h-4 max-w-[500px]' />
              <Skeleton className='h-4 max-w-[500px]' />
            </div>
          ))}
        </div>
      </div>
    );
  }

  if (error) {
    const errorMessage = get(error, "response.data.error", error.message);
    return (
      <div className='p-4'>
        <Alert variant='destructive'>
          <CircleAlert />
          <AlertTitle>Error Loading Automation</AlertTitle>
          <AlertDescription>{errorMessage}</AlertDescription>
        </Alert>
      </div>
    );
  }

  return (
    <ResizablePanelGroup direction={direction}>
      <ResizablePanel defaultSize={50} minSize={20} className={cn(!showOutput && "flex-1!")}>
        <div className='relative h-full w-full'>
          <ReactFlowProvider>
            <Suspense
              fallback={
                <div className='flex h-full w-full items-center justify-center'>
                  <LoaderCircleIcon className='animate-spin' />
                </div>
              }
            >
              <WorkflowDiagram workflowId={path} workflowConfig={workflowConfig} runId={runId} />
            </Suspense>
          </ReactFlowProvider>
          <div className='absolute right-4 bottom-4 flex items-center gap-2'>
            {!showOutput && (
              <Button variant='outline' onClick={toggleOutput} tooltip={"Show Logs Output"}>
                <LogsIcon className='h-4 w-4' />
              </Button>
            )}
          </div>

          <div className='absolute top-4 right-4 flex items-center gap-2'>
            {!!runId &&
              (isProcessing ? (
                <Button
                  variant='outline'
                  onClick={cancelRunHandler}
                  disabled={cancelRun.isPending}
                  tooltip={"Cancel Automation Run"}
                >
                  <StopCircle className='h-4 w-4' />
                </Button>
              ) : (
                <ButtonGroup>
                  <Button
                    variant='outline'
                    onClick={replayAllHandler}
                    disabled={run.isPending}
                    tooltip={"Replay Automation Run"}
                  >
                    <RotateCcw className='h-4 w-4' />
                    Replay
                  </Button>
                  {workflowConfig.variables ? (
                    <DropdownMenu>
                      <DropdownMenuTrigger asChild>
                        <Button variant='outline' className='!pl-2'>
                          <ChevronDownIcon />
                        </Button>
                      </DropdownMenuTrigger>
                      <DropdownMenuContent align='end' className='[--radius:1rem]'>
                        <DropdownMenuItem
                          onClick={() => {
                            setIsOpen(true, (data) => {
                              return run.mutateAsync({
                                workflowId: relativePath,
                                retryType: {
                                  type: "retry_with_variables",
                                  run_index: parseInt(runId, 10),
                                  replay_id: "",
                                  variables: data
                                }
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
              variant='default'
              onClick={runHandler}
              disabled={run.isPending}
              tooltip={run.isPending ? "Running..." : "Run Automation"}
              data-testid='run-workflow-button'
            >
              {run.isPending ? (
                <LoaderCircle className='animate-spin' />
              ) : (
                <PlayIcon className='h-4 w-4' />
              )}
            </Button>
            {workflowConfig.variables ? <Variables schema={variablesSchema} /> : null}
          </div>
        </div>
      </ResizablePanel>

      <ResizableHandle />

      <ResizablePanel defaultSize={50} minSize={20} className={cn(!showOutput && "flex-[unset]!")}>
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
