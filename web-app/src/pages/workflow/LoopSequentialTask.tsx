import { useState } from "react";

import { Divider } from "styled-system/jsx";

import useDiagram from "@/stores/useDiagram";

import { TaskData } from ".";
import {
  contentPaddingHeight,
  distanceBetweenHeaderAndContent,
  headerHeight,
} from "./constants";
import { TaskContainer } from "./TaskContainer";
import { TaskHeader } from "./TaskHeader";

type Props = {
  task: TaskData;
};

export function LoopSequentialTask({ task }: Props) {
  const layoutedNodes = useDiagram((state) => state.layoutedNodes);
  const setNodeVisibility = useDiagram((state) => state.setNodeVisibility);
  const nodes = useDiagram((state) => state.nodes);
  const [expanded, setExpanded] = useState(true);

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
    headerHeight + distanceBetweenHeaderAndContent + contentPaddingHeight;
  const childSpace = node.size.height - usedHeight;
  return (
    <TaskContainer>
      <TaskHeader
        task={task}
        expandable
        expanded={expanded}
        onExpandClick={onExpandClick}
      />
      {expanded && (
        <>
          <Divider color="#F5F5F5" />
          <div
            style={{
              height: `${childSpace}px`,
              width: "100%",
              display: "flex",
              flexDirection: "column",
              gap: "8px",
            }}
          ></div>
        </>
      )}
    </TaskContainer>
  );
}
