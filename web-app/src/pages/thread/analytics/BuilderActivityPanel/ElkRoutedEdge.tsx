import { BaseEdge, type EdgeProps } from "@xyflow/react";

/** Custom ReactFlow edge that renders a pre-computed SVG path from ELK edge routing. */
export const ElkRoutedEdge = ({ data, style, markerEnd }: EdgeProps) => {
  const { svgPath } = data as { svgPath: string };
  return <BaseEdge path={svgPath} style={style} markerEnd={markerEnd} />;
};

export const elkEdgeTypes = { elkRouted: ElkRoutedEdge };
