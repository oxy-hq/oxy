import type { Node } from "@xyflow/react";
import type { ContextGraphNode as ContextGraphNodeType } from "@/types/contextGraph";
import { MAX_ROW_WIDTH, MIN_NODE_WIDTH, PADDING, ROW_HEIGHT, TYPE_ORDER } from "./constants";

export function layoutRow(
  row: Array<{ node: ContextGraphNodeType; width: number }>,
  rowIndex: number
): Node[] {
  const totalWidth = row.reduce((sum, info) => sum + info.width + PADDING, 0) - PADDING;
  let x = -totalWidth / 2;

  return row.map(({ node, width }) => {
    const n: Node = {
      id: node.id,
      type: "context-graph",
      data: { label: node.label, type: node.type },
      position: { x, y: rowIndex * ROW_HEIGHT },
      zIndex: 10
    };
    x += width + PADDING;
    return n;
  });
}

export function buildInitialNodes(nodes: ContextGraphNodeType[]): Node[] {
  const typeGroups: Record<string, ContextGraphNodeType[]> = {};
  for (const node of nodes) {
    if (!typeGroups[node.type]) typeGroups[node.type] = [];
    typeGroups[node.type].push(node);
  }

  const result: Node[] = [];
  let rowIndex = 0;

  for (const type of TYPE_ORDER) {
    const nodesOfType = typeGroups[type];
    if (!nodesOfType?.length) continue;

    const nodeInfos = nodesOfType.map((node) => ({
      node,
      width: Math.max(MIN_NODE_WIDTH, node.label.length * 8 + 60)
    }));

    let currentRow: typeof nodeInfos = [];
    let currentRowWidth = 0;

    for (const info of nodeInfos) {
      const w = info.width + PADDING;
      if (currentRowWidth + w > MAX_ROW_WIDTH && currentRow.length > 0) {
        result.push(...layoutRow(currentRow, rowIndex));
        rowIndex++;
        currentRow = [info];
        currentRowWidth = w;
      } else {
        currentRow.push(info);
        currentRowWidth += w;
      }
    }
    if (currentRow.length > 0) {
      result.push(...layoutRow(currentRow, rowIndex));
      rowIndex++;
    }
  }

  return result;
}

export function buildInitialEdges(edges: Array<{ id: string; source: string; target: string }>) {
  return edges.map((edge) => ({
    id: edge.id,
    source: edge.source,
    target: edge.target,
    type: "default" as const,
    style: {
      stroke: "var(--muted-foreground)",
      strokeWidth: 1,
      opacity: 0.15
    },
    zIndex: 0
  }));
}
