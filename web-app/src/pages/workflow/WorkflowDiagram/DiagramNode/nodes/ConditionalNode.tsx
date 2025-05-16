import useWorkflow, {
  ConditionalTaskConfigWithId,
  TaskConfigWithId,
} from "@/stores/useWorkflow.ts";
import { StepContainer } from "./StepContainer";
import { NodeHeader } from "./NodeHeader";
import {
  distanceBetweenHeaderAndContent,
  headerHeight,
  nodeBorderHeight,
  paddingHeight,
} from "../../layout/constants";
import { useMemo, useState } from "react";

type Props = {
  task: TaskConfigWithId;
};

export default function ConditionalNode({ task }: Props) {
  const layoutedNodes = useWorkflow((state) => state.layoutedNodes);
  const selectedNodeId = useWorkflow((state) => state.selectedNodeId);
  const selected = selectedNodeId === task.id;
  const setNodeVisibility = useWorkflow((state) => state.setNodeVisibility);
  const nodes = useWorkflow((state) => state.nodes);
  const [expanded, setExpanded] = useState(true);
  const expandable = useMemo(() => {
    const t = task as ConditionalTaskConfigWithId;
    return t.conditions.length > 0 || t.else !== undefined;
  }, [task]);

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
    <StepContainer
      width={node.size.width}
      height={node.size.height}
      selected={selected}
    >
      <NodeHeader
        type={task.type}
        name={task.name}
        expandable={expandable}
        expanded={expanded}
        onExpandClick={onExpandClick}
      />
      <>
        <div style={{ height: `${childSpace}px` }}></div>
      </>
    </StepContainer>
  );
}
