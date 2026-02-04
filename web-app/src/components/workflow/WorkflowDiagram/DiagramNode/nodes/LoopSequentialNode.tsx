import { useMemo } from "react";
import { Combobox, type ComboboxStyles } from "@/components/ui/shadcn/combobox";
import { NodeAppendix } from "@/components/ui/shadcn/node-appendix";
import { useSelectedLoopIndex } from "@/components/workflow/useWorkflowRun";
import type { TaskRun } from "@/services/types";
import { useBlockStore } from "@/stores/block";
import useWorkflow, {
  type LoopSequentialTaskConfig,
  type TaskConfigWithId
} from "@/stores/useWorkflow";
import {
  distanceBetweenHeaderAndContent,
  headerHeight,
  nodeBorderHeight,
  paddingHeight
} from "../../layout/constants";
import { NodeHeader } from "./NodeHeader";

type Props = {
  parentId?: string;
  task: TaskConfigWithId;
  taskRun?: TaskRun;
  loopRuns?: TaskRun[];
  expanded?: boolean;
};

export function LoopSequentialNode({ parentId, task, taskRun, loopRuns, expanded }: Props) {
  const nodes = useWorkflow((state) => state.nodes);
  const setNodeExpanded = useWorkflow((state) => state.setNodeExpanded);
  const tasks = (task as LoopSequentialTaskConfig).tasks;
  const expandable = useMemo(() => tasks.length > 0, [tasks]);
  const setSelectedLoopIndex = useBlockStore((state) => state.setSelectedLoopIndex);
  const selectedLoopIndex = useSelectedLoopIndex(task);
  const parentNode = nodes.find((n) => n.id === parentId);
  const appendixPosition = parentNode?.data.task.type === "loop_sequential" ? "left" : "right";

  const node = nodes.find((n) => n.id === task.id);
  const onExpandClick = () => {
    setNodeExpanded(task.id, !expanded);
  };
  if (!node || !node.height) return null;
  const usedHeight =
    headerHeight + distanceBetweenHeaderAndContent + paddingHeight + nodeBorderHeight;
  const childSpace = node.height - usedHeight;
  return (
    <>
      {!!taskRun?.loopValue?.length && expanded ? (
        <NodeAppendix position={appendixPosition}>
          <p className='pb-2 text-muted-foreground text-sm'>Loop value</p>
          <Combobox
            value={selectedLoopIndex?.toString()}
            onValueChange={(value) => setSelectedLoopIndex(task, +value)}
            items={taskRun.loopValue.map((value, index) => ({
              label: `${JSON.parse(JSON.stringify(value))}`,
              value: `${index}`,
              style: loopRuns
                ?.filter((run) => run.id.endsWith(`-${index}`))
                .reduce(
                  (_acc, run) => {
                    if (run.error) {
                      return "error";
                    }
                    if (run.isStreaming) {
                      return "loading";
                    }
                    return "success";
                  },
                  undefined as ComboboxStyles | undefined
                )
            }))}
          />
        </NodeAppendix>
      ) : null}
      <NodeHeader
        name={task.name}
        type={task.type}
        expandable={expandable}
        expanded={expanded}
        onExpandClick={onExpandClick}
      />
      {expandable && expanded && <div style={{ height: `${childSpace}px` }}></div>}
    </>
  );
}
