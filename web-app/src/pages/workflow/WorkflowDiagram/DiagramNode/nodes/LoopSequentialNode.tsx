import { useMemo, useState } from "react";

import useWorkflow, {
  LoopSequentialTaskConfig,
  TaskConfigWithId,
} from "@/stores/useWorkflow";
import {
  distanceBetweenHeaderAndContent,
  headerHeight,
  nodeBorderHeight,
  paddingHeight,
} from "../../layout/constants";
import { StepContainer } from "./StepContainer";
import { NodeHeader } from "./NodeHeader";

type Props = {
  task: TaskConfigWithId;
};

export function LoopSequentialNode({ task }: Props) {
  const layoutedNodes = useWorkflow((state) => state.layoutedNodes);
  const selectedNodeId = useWorkflow((state) => state.selectedNodeId);
  const selected = selectedNodeId === task.id;
  const setNodeVisibility = useWorkflow((state) => state.setNodeVisibility);
  const nodes = useWorkflow((state) => state.nodes);
  const tasks = (task as LoopSequentialTaskConfig).tasks;
  const [expanded, setExpanded] = useState(true);
  const expandable = useMemo(() => tasks.length > 0, [tasks]);

  const node = layoutedNodes.find((n) => n.id === task.id);
  const onExpandClick = () => {
    const children = nodes
      .filter((n) => n.parentId === task.id)
      .map((n) => n.id);
    setNodeVisibility(children, !expanded);
    setExpanded(!expanded);
  };
  if (!node) return null;
  const usedHeight =
    headerHeight +
    distanceBetweenHeaderAndContent +
    paddingHeight +
    nodeBorderHeight;
  const childSpace = node.size.height - usedHeight;
  return (
    <StepContainer selected={selected}>
      <NodeHeader
        name={task.name}
        type={task.type}
        expandable={expandable}
        expanded={expanded}
        onExpandClick={onExpandClick}
      />
      {expandable && expanded && (
        <>
          <div style={{ height: `${childSpace}px` }}></div>
        </>
      )}
    </StepContainer>
  );
}
