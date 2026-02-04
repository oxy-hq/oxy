import { useMemo } from "react";
import useWorkflow, {
  type ConditionalTaskConfigWithId,
  type TaskConfigWithId
} from "@/stores/useWorkflow.ts";
import {
  distanceBetweenHeaderAndContent,
  headerHeight,
  nodeBorderHeight,
  paddingHeight
} from "../../layout/constants";
import { NodeHeader } from "./NodeHeader";

type Props = {
  task: TaskConfigWithId;
  expanded?: boolean;
};

export default function ConditionalNode({ task, expanded }: Props) {
  const layoutedNodes = useWorkflow((state) => state.nodes);
  const setNodeExpanded = useWorkflow((state) => state.setNodeExpanded);
  const expandable = useMemo(() => {
    const t = task as ConditionalTaskConfigWithId;
    return t.conditions.length > 0 || t.else !== undefined;
  }, [task]);

  const node = layoutedNodes.find((n) => n.id === task.id);
  const onExpandClick = () => {
    setNodeExpanded(task.id, !expanded);
  };
  if (!node || !node.height) return null;
  const usedHeight =
    headerHeight + distanceBetweenHeaderAndContent + paddingHeight + nodeBorderHeight;
  const childSpace = node.height - usedHeight;
  return (
    <>
      <NodeHeader
        type={task.type}
        name={task.name}
        expandable={expandable}
        expanded={expanded}
        onExpandClick={onExpandClick}
      />
      <div style={{ height: `${childSpace}px` }}></div>
    </>
  );
}
