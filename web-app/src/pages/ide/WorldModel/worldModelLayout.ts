import { Position, type Edge as RFEdge, type Node as RFNode } from "@xyflow/react";
import ELK from "elkjs";
import type { WmSelection, WorldModel } from "@/types/worldModel";

const elk = new ELK();

export const NODE_WIDTH = 184;
/** 3-row compact card: name row + grain/depth row + obs/calc row + padding. */
export const NODE_HEIGHT_COLLAPSED = 80;
/** Width of the in-place expanded entity node (measure-tree mode). */
export const EXPANDED_NODE_WIDTH = 360;

/** Map a measure_type string to a display symbol. */
export function measureSymbol(type: string): string {
  switch (type) {
    case "sum":
      return "Σ";
    case "count":
    case "count_distinct":
    case "count_distinct_approx":
      return "#";
    case "average":
    case "median":
      return "⌀";
    case "min":
      return "↓";
    case "max":
      return "↑";
    default:
      return "ƒ";
  }
}

/** Map a measure_type string to a Tailwind text-color class. */
export function measureSymbolColor(_type: string): string {
  return "text-[color:var(--vis-purple)]";
}

/** Derive which entity IDs and edge keys are "active" given the current selection. */
function deriveActive(
  model: WorldModel,
  selection: WmSelection
): { activeEntityIds: Set<string> | null; activeEdgeKeys: Set<string> } {
  if (!selection) return { activeEntityIds: null, activeEdgeKeys: new Set() };

  const activeEntityIds = new Set<string>();
  const activeEdgeKeys = new Set<string>();

  if (
    selection.kind === "entity" ||
    selection.kind === "dimension" ||
    selection.kind === "measure" ||
    selection.kind === "instance"
  ) {
    const id = selection.entityId;
    activeEntityIds.add(id);

    if (selection.kind === "instance") {
      // BFS: traverse the full connected subgraph so the entire hierarchy
      // chain is active, not just immediate neighbors of the seed entity.
      const queue = [id];
      while (queue.length > 0) {
        const current = queue.shift()!;
        for (const e of model.edges) {
          if (e.from === current || e.to === current) {
            activeEdgeKeys.add(`${e.from}->${e.to}`);
            const neighbor = e.from === current ? e.to : e.from;
            if (!activeEntityIds.has(neighbor)) {
              activeEntityIds.add(neighbor);
              queue.push(neighbor);
            }
          }
        }
      }
    } else {
      for (const e of model.edges) {
        if (e.from === id || e.to === id) {
          activeEdgeKeys.add(`${e.from}->${e.to}`);
          activeEntityIds.add(e.from);
          activeEntityIds.add(e.to);
        }
      }
    }
  } else if (selection.kind === "promotion") {
    activeEntityIds.add(selection.from);
    activeEntityIds.add(selection.to);
    activeEdgeKeys.add(`${selection.from}->${selection.to}`);
  }

  return { activeEntityIds, activeEdgeKeys };
}

/** Convert a WorldModel to unpositioned React Flow nodes and edges.
 *  filterCounts is intentionally excluded — apply it separately after layout
 *  so count updates don't trigger expensive ELK re-layout. */
export function worldModelToFlow(
  model: WorldModel,
  selection: WmSelection
): { nodes: RFNode[]; edges: RFEdge[] } {
  const { activeEntityIds, activeEdgeKeys } = deriveActive(model, selection);
  const hasSelection = activeEntityIds !== null;

  const primaryId =
    selection?.kind === "entity"
      ? selection.entityId
      : selection?.kind === "dimension" || selection?.kind === "measure"
        ? selection.entityId
        : selection?.kind === "instance"
          ? selection.entityId
          : null;

  const nodes: RFNode[] = model.entities.map((entity) => {
    const isActive = !hasSelection || (activeEntityIds?.has(entity.id) ?? true);
    return {
      id: entity.id,
      type: "wm-entity",
      position: { x: 0, y: 0 },
      sourcePosition: Position.Bottom,
      targetPosition: Position.Top,
      data: {
        entity,
        selected: entity.id === primaryId,
        dimmed: !isActive,
        filterCount: null
      },
      width: NODE_WIDTH,
      height: NODE_HEIGHT_COLLAPSED
    };
  });

  const edges: RFEdge[] = model.edges.map((edge, i) => {
    const key = `${edge.from}->${edge.to}`;
    const isActive = !hasSelection || activeEdgeKeys.has(key);
    const isHighlighted =
      selection?.kind === "promotion" && selection.from === edge.from && selection.to === edge.to;
    const isFanout = !edge.functional;

    const stroke = isFanout
      ? "var(--destructive)"
      : isHighlighted || isActive
        ? "var(--info)"
        : "var(--info)";

    return {
      id: `${key}#${i}`,
      source: edge.from,
      target: edge.to,
      type: "smoothstep",
      ...(isHighlighted
        ? {
            label: isFanout ? "⇉" : "Σp",
            labelStyle: {
              fontFamily: "ui-monospace, monospace",
              fontSize: 11,
              fill: isFanout ? "var(--destructive)" : "var(--info)",
              fontWeight: 500
            },
            labelBgStyle: { fill: "var(--card)" },
            labelBgPadding: [3, 5] as [number, number],
            labelBgBorderRadius: 2
          }
        : {}),
      // Flow the direction on edges connected to the current selection — both
      // when an edge is clicked (promotion) and when a node is clicked.
      animated: isHighlighted || (hasSelection && isActive),
      style: {
        stroke,
        strokeWidth: isHighlighted ? 2 : 1,
        strokeDasharray: isFanout ? "4 3" : undefined,
        opacity: isHighlighted ? 0.95 : isActive ? 0.6 : 0.08,
        transition: "opacity 0.2s, stroke 0.2s"
      }
    };
  });

  return { nodes, edges };
}

/** Ordered waypoints (ELK section start + bendPoints + end) per ReactFlow edge ID. */
export type WaypointMap = Map<string, { x: number; y: number }[]>;

/** Position nodes with ELK layered layout + SPLINES edge routing.
 *  Edges are forced to exit the bottom of source nodes and enter the top
 *  of target nodes via explicit SOUTH/NORTH port constraints. */
export async function layoutWithElk(
  nodes: RFNode[],
  edges: RFEdge[]
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
    children: nodes.map((n) => ({
      id: n.id,
      width: NODE_WIDTH,
      height: NODE_HEIGHT_COLLAPSED,
      ports: [
        { id: `${n.id}__src`, properties: { "port.side": "SOUTH" } },
        { id: `${n.id}__tgt`, properties: { "port.side": "NORTH" } }
      ],
      properties: { portConstraints: "FIXED_SIDE" }
    })),
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
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  for (const elkEdge of (result.edges ?? []) as any[]) {
    const section = elkEdge.sections?.[0];
    if (!section) continue;
    const pts: { x: number; y: number }[] = [];
    if (section.startPoint) pts.push({ x: section.startPoint.x, y: section.startPoint.y });
    for (const bp of section.bendPoints ?? []) pts.push({ x: bp.x, y: bp.y });
    if (section.endPoint) pts.push({ x: section.endPoint.x, y: section.endPoint.y });
    if (pts.length >= 2) waypointMap.set(elkEdge.id as string, pts);
  }

  return { nodes: positionedNodes, waypointMap };
}
