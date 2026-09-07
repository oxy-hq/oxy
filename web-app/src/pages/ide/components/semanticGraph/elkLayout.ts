import type { Edge as RFEdge, Node as RFNode } from "@xyflow/react";
import ELK from "elkjs";
import { NODE_HEIGHT_COLLAPSED, NODE_WIDTH } from "./constants";

const elk = new ELK();

/** Ordered waypoints (ELK section start + bendPoints + end) per ReactFlow edge ID. */
export type WaypointMap = Map<string, { x: number; y: number }[]>;

/** A node's box size handed to ELK. */
export interface NodeSize {
  width: number;
  height: number;
}

/** Per-node layout size lookup. Returns the size ELK should reserve for a node
 *  in the CURRENT display state — a grown (expanded / measure-chip / sample)
 *  card is bigger than a collapsed one, so neighbors reflow to make room instead
 *  of being overlapped. Falls back to the collapsed box when a node isn't
 *  covered. */
export type NodeSizeOf = (id: string) => NodeSize | undefined;

interface Point {
  x: number;
  y: number;
}

/** What ELK hands back per edge once it has routed it. */
interface RoutedElkEdge {
  id: string;
  sections?: { startPoint?: Point; endPoint?: Point; bendPoints?: Point[] }[];
}

/**
 * Position nodes with ELK layered layout + orthogonal edge routing.
 * Edges are forced to exit the bottom of source nodes and enter the top
 * of target nodes via explicit SOUTH/NORTH port constraints.
 *
 * `sizeOf` supplies each node's real (possibly grown) box so the layout
 * reserves room for expanded/measure/sample cards; without it every node is
 * laid out at the collapsed size.
 *
 * The returned `waypointMap` is what lets edges route *around* node bodies
 * instead of cutting through them — render it via {@link GraphEdge}. A caller
 * that ignores it gets React Flow's own router and a different-looking graph,
 * which is exactly the drift this module exists to prevent.
 */
export async function layoutWithElk(
  nodes: RFNode[],
  edges: RFEdge[],
  sizeOf?: NodeSizeOf
): Promise<{ nodes: RFNode[]; waypointMap: WaypointMap }> {
  if (nodes.length === 0) return { nodes, waypointMap: new Map() };

  const graph = {
    id: "root",
    layoutOptions: {
      "elk.algorithm": "layered",
      "elk.direction": "DOWN",
      "elk.edgeRouting": "ORTHOGONAL",
      "elk.layered.spacing.nodeNodeBetweenLayers": "100",
      "elk.spacing.nodeNode": "64",
      "elk.separateConnectedComponents": "true",
      "elk.spacing.componentComponent": "100",
      "elk.layered.nodePlacement.strategy": "NETWORK_SIMPLEX",
      "elk.layered.crossingMinimization.strategy": "LAYER_SWEEP",
      "elk.layered.crossingMinimization.greedySwitchType": "TWO_SIDED",
      "elk.layered.thoroughness": "50",
      "elk.layered.unnecessaryBendpoints": "true",
      "elk.layered.cycleBreaking.strategy": "GREEDY",
      "elk.layered.nodePlacement.bk.fixedAlignment": "BALANCED",
      "elk.layered.spacing.edgeNodeBetweenLayers": "40",
      "elk.layered.spacing.edgeEdgeBetweenLayers": "20"
    },
    children: nodes.map((n) => {
      const size = sizeOf?.(n.id) ?? { width: NODE_WIDTH, height: NODE_HEIGHT_COLLAPSED };
      return {
        id: n.id,
        width: size.width,
        height: size.height,
        ports: [
          { id: `${n.id}__src`, properties: { "port.side": "SOUTH" } },
          { id: `${n.id}__tgt`, properties: { "port.side": "NORTH" } }
        ],
        properties: { portConstraints: "FIXED_SIDE" }
      };
    }),
    edges: edges.map((e) => ({
      id: e.id,
      sources: [`${e.source}__src`],
      targets: [`${e.target}__tgt`]
    }))
  };

  const result = await elk.layout(graph);
  const positions = new Map((result.children ?? []).map((c) => [c.id, c]));

  const positionedNodes = nodes.map((node) => {
    const pos = positions.get(node.id);
    return pos ? { ...node, position: { x: pos.x ?? 0, y: pos.y ?? 0 } } : node;
  });

  const waypointMap: WaypointMap = new Map();
  // ELK infers the result edge type from the plain graph literal above, which
  // has no `sections` — they exist only on what the layout returns.
  const routedEdges = (result.edges ?? []) as unknown as RoutedElkEdge[];
  for (const elkEdge of routedEdges) {
    const section = elkEdge.sections?.[0];
    if (!section) continue;
    const pts: { x: number; y: number }[] = [];
    if (section.startPoint) pts.push({ x: section.startPoint.x, y: section.startPoint.y });
    for (const bp of section.bendPoints ?? []) pts.push({ x: bp.x, y: bp.y });
    if (section.endPoint) pts.push({ x: section.endPoint.x, y: section.endPoint.y });
    if (pts.length >= 2) waypointMap.set(elkEdge.id, pts);
  }

  return { nodes: positionedNodes, waypointMap };
}
