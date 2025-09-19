import { Handle, NodeProps, NodeToolbar, Position } from "@xyflow/react";

import { NodeContent } from "./NodeContent";
import { NodeData, NodeType } from "@/stores/useWorkflow";
import {
  NodeStatus,
  NodeStatusIndicator,
} from "@/components/ui/shadcn/node-status-indicator";
import { StepContainer } from "./nodes/StepContainer";
import {
  useIsProcessing,
  useTaskRun,
  useWorkflowRun,
} from "../../useWorkflowRun";
import { RefreshCcw } from "lucide-react";
import { Button } from "@/components/ui/shadcn/button";

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
  parentId,
}: Props) {
  const task = data.task;

  const { taskRun, taskRunId, runId, loopRuns } = useTaskRun(task);
  const isProcessing = useIsProcessing(task.workflowId, task.runId || "");
  const runWorkflow = useWorkflowRun();

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
    <NodeStatusIndicator status={nodeStatus} variant="border" key={id}>
      <NodeToolbar
        className="flex items-center justify-between"
        isVisible={
          (nodeStatus === "error" || nodeStatus === "success") &&
          !!selected &&
          !isProcessing
        }
      >
        <Button
          tooltip={"Replay this step"}
          size="icon"
          onClick={() => {
            if (!runId) {
              return;
            }
            runWorkflow.mutate({
              workflowId: task.workflowId,
              retryParam: {
                run_id: +runId,
                replay_id: taskRunId,
              },
            });
          }}
        >
          <RefreshCcw />
        </Button>
      </NodeToolbar>
      <Handle
        type="target"
        position={Position.Top}
        isConnectable={isConnectable}
        className="invisible !top-0.5"
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
        type="source"
        position={Position.Bottom}
        isConnectable={isConnectable}
        className="invisible !bottom-0.5"
      />
    </NodeStatusIndicator>
  );
}
