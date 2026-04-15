import { Handle, type NodeProps, NodeToolbar, Position } from "@xyflow/react";
import { RefreshCcw } from "lucide-react";
import { Button } from "@/components/ui/shadcn/button";
import { type NodeStatus, NodeStatusIndicator } from "@/components/ui/shadcn/node-status-indicator";
import type { NodeData, NodeType } from "@/stores/useWorkflow";
import { useIsProcessing, useStreamEvents, useTaskRun, useWorkflowRun } from "../../useWorkflowRun";
import { NodeContent } from "./NodeContent";
import { StepContainer } from "./nodes/StepContainer";

type Node = {
  id: string;
  data: NodeData;
  position: {
    x: number;
    y: number;
  };
  type: NodeType;
  parentId?: string;
  width?: number;
  height?: number;
  sourcePosition?: Position;
  targetPosition?: Position;
  dragHandle?: string;
};

type Props = NodeProps<Node>;

export function DiagramNode({
  id,
  data,
  isConnectable,
  type,
  width,
  height,
  selected,
  parentId
}: Props) {
  const task = data.task;

  const { taskRun, taskRunId, runId, loopRuns } = useTaskRun(task);
  const isProcessing = useIsProcessing(task.workflowId, task.runId || "");
  const runWorkflow = useWorkflowRun();
  const { stream } = useStreamEvents();

  let nodeStatus: NodeStatus = "initial";
  if (taskRun) {
    if (taskRun.error) {
      nodeStatus = "error";
    } else if (taskRun.isStreaming) {
      nodeStatus = "loading";
    } else {
      nodeStatus = "success";
    }
  }

  return (
    <NodeStatusIndicator status={nodeStatus} variant='border' key={id}>
      <NodeToolbar
        className='flex items-center justify-between'
        isVisible={
          (nodeStatus === "error" || nodeStatus === "success") && !!selected && !isProcessing
        }
      >
        <Button
          variant='ghost'
          tooltip={"Replay this step"}
          size='icon'
          onClick={async () => {
            if (!runId) {
              return;
            }
            try {
              await runWorkflow.mutateAsync({
                workflowId: task.workflowId,
                retryType: {
                  type: "retry",
                  run_index: +runId,
                  replay_id: taskRunId
                }
              });
              // Manually trigger streaming after the replay starts.
              // This is necessary because the run_index might be the same,
              // so the URL won't change and the auto-stream useEffect won't re-trigger.
              await stream.mutateAsync({
                sourceId: task.workflowId,
                runIndex: +runId
              });
            } catch (error) {
              console.error("Failed to replay from step:", error);
            }
          }}
        >
          <RefreshCcw />
        </Button>
      </NodeToolbar>
      <Handle
        type='target'
        position={Position.Top}
        isConnectable={isConnectable}
        className='invisible top-0.5!'
      />
      <StepContainer selected={!!selected}>
        <NodeContent
          id={id}
          parentId={parentId}
          task={data.task}
          taskRun={taskRun}
          loopRuns={loopRuns}
          data={data}
          type={type}
          width={width}
          height={height}
        />
      </StepContainer>
      <Handle
        type='source'
        position={Position.Bottom}
        isConnectable={isConnectable}
        className='invisible bottom-0.5!'
      />
    </NodeStatusIndicator>
  );
}
