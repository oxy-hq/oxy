import { useState } from "react";

import { Divider } from "styled-system/jsx";

import useDiagram from "@/stores/useDiagram";

import { StepData } from ".";
import {
  contentPaddingHeight,
  distanceBetweenHeaderAndContent,
  headerHeight,
} from "./constants";
import { StepContainer } from "./StepContainer";
import { StepHeader } from "./StepHeader";

type Props = {
  step: StepData;
};

export function LoopSequentialStep({ step }: Props) {
  const layoutedNodes = useDiagram((state) => state.layoutedNodes);
  const setNodeVisibility = useDiagram((state) => state.setNodeVisibility);
  const nodes = useDiagram((state) => state.nodes);
  const [expanded, setExpanded] = useState(true);

  const node = layoutedNodes.find((n) => n.id === step.id);
  const onExpandClick = () => {
    const children = nodes
      .filter((n) => n.parentId === step.id)
      .map((n) => n.id);
    setNodeVisibility(children, !expanded);
    setExpanded(!expanded);
  };
  if (!node) return null;
  const usedHeight =
    headerHeight + distanceBetweenHeaderAndContent + contentPaddingHeight;
  const childSpace = node.size.height - usedHeight;
  return (
    <StepContainer>
      <StepHeader
        step={step}
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
    </StepContainer>
  );
}
