import { TaskNode } from "@/stores/useWorkflow";
import { computeNodeDimensions } from "./nodeSize";
import { createElkLayout } from "./elkLayout";
import { Edge } from "@xyflow/react";

export const calculateNodesSize = (nodes: TaskNode[]): TaskNode[] => {
  const nodesWithSize = nodes.map((node) => ({ ...node }));

  nodesWithSize.forEach((node) => {
    computeNodeDimensions(node, nodesWithSize);
  });

  const maxWidth = Math.max(...nodesWithSize.map((node) => node.width || 0));
  nodesWithSize
    .filter((node) => !node.parentId)
    .forEach((node) => {
      node.width = maxWidth;
    });

  return nodesWithSize;
};

export const getLayoutedElements = async (
  nodes: TaskNode[],
  edges: Edge[],
): Promise<TaskNode[]> => {
  return createElkLayout(nodes, edges);
};
