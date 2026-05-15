import { Handle, type NodeProps, NodeToolbar, Position } from "@xyflow/react";
import { RefreshCcw } from "lucide-react";
import { Button } from "@/components/ui/shadcn/button";
import { type NodeStatus, NodeStatusIndicator } from "@/components/ui/shadcn/node-status-indicator";
import {
  type StepStatus,
  useReplayStep,
  useStepStatus
} from "@/pages/workflow/components/RunStatusContext";
import type { NodeData, NodeType } from "@/stores/useWorkflow";
import { NodeContent } from "./NodeContent";
import { StepContainer } from "./nodes/StepContainer";

// Map the new event-stream step status onto the diagram's existing
// 4-state border styling. `cached` reuses `success` because the
// "reused from prior run" cue lives in the StepList beside the
// diagram — keeping the border visual consistent avoids a third
// border colour the user has to learn. `skipped` reuses `initial`:
// downstream-of-failure steps never ran, so the neutral default
// border (no extra colour) reads accurately as "didn't get its turn".
const stepStatusToNodeStatus = (s: StepStatus): NodeStatus => {
  switch (s) {
    case "running":
      return "loading";
    case "success":
    case "cached":
      return "success";
    case "failed":
      return "error";
    case "pending":
    case "skipped":
      return "initial";
  }
};

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

  // Status + replay both come from the workflow run page's
  // `RunStatusProvider`. The legacy `useWorkflowRun` / `useTaskRun`
  // fallbacks were only reachable from `WorkflowPreview`, which is
  // gone; the new `Workflow` component always mounts the provider.
  const liveStatus = useStepStatus(task.name);
  const replayStep = useReplayStep();
  const nodeStatus: NodeStatus = liveStatus ? stepStatusToNodeStatus(liveStatus.status) : "initial";
  const isRunning = liveStatus?.status === "running";
  const toolbarVisible =
    !!selected && !isRunning && (nodeStatus === "error" || nodeStatus === "success");

  return (
    <NodeStatusIndicator status={nodeStatus} variant='border' key={id}>
      <NodeToolbar className='flex items-center justify-between' isVisible={toolbarVisible}>
        <Button
          variant='ghost'
          tooltip={"Replay this step"}
          size='icon'
          disabled={!replayStep}
          onClick={async () => {
            if (!replayStep) return;
            try {
              await replayStep(task.name);
            } catch (error) {
              console.error("Failed to replay step:", error);
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
