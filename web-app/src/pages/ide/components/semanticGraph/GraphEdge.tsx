import { BaseEdge, type EdgeProps, getSmoothStepPath } from "@xyflow/react";

interface GraphEdgeData extends Record<string, unknown> {
  waypoints?: { x: number; y: number }[];
}

/** Renders an orthogonal path with rounded corners through ELK-computed
 *  waypoints; edges added after layout (e.g. World Model's breakdown edges)
 *  carry no waypoints and fall back to React Flow's smooth-step router so they
 *  route with the same right-angle, rounded look instead of a raw diagonal. */
export function GraphEdge({
  sourceX,
  sourceY,
  sourcePosition,
  targetX,
  targetY,
  targetPosition,
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
  const waypoints = (data as GraphEdgeData)?.waypoints;
  let path: string;
  let labelX = (sourceX + targetX) / 2;
  let labelY = (sourceY + targetY) / 2;

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
    const mid = pts[Math.floor(pts.length / 2)];
    labelX = mid.x;
    labelY = mid.y;
  } else {
    const [smoothPath, lx, ly] = getSmoothStepPath({
      sourceX,
      sourceY,
      sourcePosition,
      targetX,
      targetY,
      targetPosition,
      borderRadius: 10
    });
    path = smoothPath;
    labelX = lx;
    labelY = ly;
  }

  return (
    <BaseEdge
      path={path}
      labelX={labelX}
      labelY={labelY}
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
