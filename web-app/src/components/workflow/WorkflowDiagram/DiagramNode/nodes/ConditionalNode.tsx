import useWorkflow, {
  ConditionalTaskConfigWithId,
  TaskConfigWithId,
} from "@/stores/useWorkflow.ts";
import { NodeHeader } from "./NodeHeader";
import {
  distanceBetweenHeaderAndContent,
  headerHeight,
  nodeBorderHeight,
  paddingHeight,
} from "../../layout/constants";
import { useMemo } from "react";

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
    headerHeight +
    distanceBetweenHeaderAndContent +
    paddingHeight +
    nodeBorderHeight;
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
