import useWorkflow, {
  TaskConfigWithId,
  WorkflowTaskConfigWithId,
} from "@/stores/useWorkflow";
import { NodeHeader } from "./NodeHeader";
import { TaskRun } from "@/services/types";
import { useMemo, useState } from "react";
import {
  distanceBetweenHeaderAndContent,
  headerHeight,
  nodeBorderHeight,
  paddingHeight,
} from "../../layout/constants";

type Props = {
  task: TaskConfigWithId;
  taskRun?: TaskRun;
};

export function WorkflowTaskNode({ task, taskRun }: Props) {
  const layoutedNodes = useWorkflow((state) => state.layoutedNodes);
  const setNodeVisibility = useWorkflow((state) => state.setNodeVisibility);
  const nodes = useWorkflow((state) => state.nodes);
  const tasks = (task as WorkflowTaskConfigWithId).tasks;
  const [expanded, setExpanded] = useState(true);
  const expandable = useMemo(() => !!tasks && tasks.length > 0, [tasks]);

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
    <>
      <NodeHeader
        name={task.name}
        type={task.type}
        task={task}
        taskRun={taskRun}
        expandable={expandable}
        expanded={expanded}
        onExpandClick={onExpandClick}
      />
      {expandable && expanded && (
        <div style={{ height: `${childSpace}px` }}></div>
      )}
    </>
  );
}
