import useWorkflow, {
  TaskConfigWithId,
  WorkflowTaskConfigWithId,
} from "@/stores/useWorkflow";
import { NodeHeader } from "./NodeHeader";
import { TaskRun } from "@/services/types";
import { useMemo } from "react";
import {
  distanceBetweenHeaderAndContent,
  headerHeight,
  nodeBorderHeight,
  paddingHeight,
} from "../../layout/constants";

type Props = {
  task: TaskConfigWithId;
  taskRun?: TaskRun;
  expanded?: boolean;
};

export function WorkflowTaskNode({ task, taskRun, expanded }: Props) {
  const nodes = useWorkflow((state) => state.nodes);
  const setNodeExpanded = useWorkflow((state) => state.setNodeExpanded);
  const tasks = (task as WorkflowTaskConfigWithId).tasks;
  const expandable = useMemo(() => !!tasks && tasks.length > 0, [tasks]);

  const node = nodes.find((n) => n.id === task.id);
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
