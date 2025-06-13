import { Node, Edge, LayoutedNode } from "@/stores/useWorkflow";
import { computeNodeDimensions } from "./nodeSize";
import { createElkLayout } from "./elkLayout";

export const calculateNodesSize = (nodes: Node[]): Node[] => {
  const nodesWithSize = nodes.map((node) => ({ ...node }));

  nodesWithSize.forEach((node) => {
    computeNodeDimensions(node, nodesWithSize);
  });

  const maxWidth = Math.max(...nodesWithSize.map((node) => node.width));
  nodesWithSize
    .filter((node) => !node.parentId)
    .forEach((node) => {
      node.width = maxWidth;
    });

  return nodesWithSize;
};

export const getLayoutedElements = async (
  nodes: Node[],
  edges: Edge[],
): Promise<LayoutedNode[]> => {
  return createElkLayout(nodes, edges);
};
