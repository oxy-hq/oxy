import type { Edge as RFEdge, Node as RFNode } from "@xyflow/react";
import ELK from "elkjs";
import type { MetricTree } from "@/types/metricTree";

export const NODE_WIDTH = 200;
export const NODE_HEIGHT = 72;

const elk = new ELK();

/**
 * Convert a `MetricTree` into React Flow nodes and edges (unpositioned).
 * Component edges render solid; driver edges render dashed and accented.
 */
export function metricTreeToFlow(
  tree: MetricTree,
  selectedId: string | null,
  roles?: Map<string, string>
): { nodes: RFNode[]; edges: RFEdge[] } {
  const nodes: RFNode[] = tree.nodes.map((node) => ({
    id: node.id,
    type: "metric-measure",
    position: { x: 0, y: 0 },
    data: { node, selected: node.id === selectedId, role: roles?.get(node.id) ?? "leaf" },
    width: NODE_WIDTH,
    height: NODE_HEIGHT
  }));

  const edges: RFEdge[] = tree.edges.map((edge, index) => {
    const isDriver = edge.kind === "driver";
    return {
      id: `${edge.from}->${edge.to}#${index}`,
      source: edge.from,
      target: edge.to,
      type: "smoothstep",
      animated: isDriver,
      data: { kind: edge.kind },
      style: {
        stroke: isDriver ? "var(--primary)" : "var(--success)",
        strokeWidth: isDriver ? 1.5 : 2,
        strokeDasharray: isDriver ? "6 4" : undefined,
        opacity: 0.7
      }
    };
  });

  return { nodes, edges };
}

/** Position nodes with an ELK top-down layered layout. */
export async function layoutWithElk(nodes: RFNode[], edges: RFEdge[]): Promise<RFNode[]> {
  if (nodes.length === 0) return nodes;

  const graph = {
    id: "root",
    layoutOptions: {
      "elk.algorithm": "layered",
      "elk.direction": "DOWN",
      "elk.layered.spacing.nodeNodeBetweenLayers": "80",
      "elk.spacing.nodeNode": "48"
    },
    children: nodes.map((n) => ({ id: n.id, width: NODE_WIDTH, height: NODE_HEIGHT })),
    edges: edges.map((e) => ({ id: e.id, sources: [e.source], targets: [e.target] }))
  };

  const result = await elk.layout(graph);
  const positions = new Map((result.children ?? []).map((c) => [c.id, c]));

  return nodes.map((node) => {
    const pos = positions.get(node.id);
    return pos ? { ...node, position: { x: pos.x ?? 0, y: pos.y ?? 0 } } : node;
  });
}
