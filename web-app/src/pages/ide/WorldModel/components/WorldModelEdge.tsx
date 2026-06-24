import { BaseEdge, type EdgeProps } from "@xyflow/react";

interface WmEdgeData extends Record<string, unknown> {
  waypoints?: { x: number; y: number }[];
}

/** Renders a smooth bezier curve through ELK-computed waypoints. */
export function WorldModelEdge({
  sourceX,
  sourceY,
  targetX,
  targetY,
  data,
  style,
  label,
  labelStyle,
  labelBgStyle,
  labelBgPadding,
  labelBgBorderRadius,
  markerEnd,
  markerStart,
  interactionWidth
}: EdgeProps) {
  const waypoints = (data as WmEdgeData)?.waypoints;
  let path: string;

  if (waypoints && waypoints.length >= 2) {
    // Orthogonal path with rounded corners at each bend.
    const pts = waypoints;
    const r = 10;
    let d = `M ${pts[0].x} ${pts[0].y}`;
    for (let i = 1; i < pts.length - 1; i++) {
      const prev = pts[i - 1];
      const curr = pts[i];
      const next = pts[i + 1];
      const dx1 = Math.sign(curr.x - prev.x);
      const dy1 = Math.sign(curr.y - prev.y);
      const dx2 = Math.sign(next.x - curr.x);
      const dy2 = Math.sign(next.y - curr.y);
      d += ` L ${curr.x - dx1 * r} ${curr.y - dy1 * r}`;
      d += ` Q ${curr.x} ${curr.y} ${curr.x + dx2 * r} ${curr.y + dy2 * r}`;
    }
    d += ` L ${pts[pts.length - 1].x} ${pts[pts.length - 1].y}`;
    path = d;
  } else {
    path = `M ${sourceX} ${sourceY} L ${targetX} ${targetY}`;
  }

  return (
    <BaseEdge
      path={path}
      style={style}
      label={label}
      labelStyle={labelStyle}
      labelBgStyle={labelBgStyle}
      labelBgPadding={labelBgPadding}
      labelBgBorderRadius={labelBgBorderRadius}
      markerEnd={markerEnd}
      markerStart={markerStart}
      interactionWidth={interactionWidth}
    />
  );
}
