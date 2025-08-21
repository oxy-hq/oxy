import ELK, { ElkExtendedEdge, ElkNode } from "elkjs";
import { TaskNode as Node } from "@/stores/useWorkflow";
import {
  contentPadding,
  distanceBetweenHeaderAndContent,
  distanceBetweenNodes,
  headerHeight,
  nodeBorder,
  nodePadding,
} from "./constants";
import { Edge } from "@xyflow/react";

const elk = new ELK();

export const createElkLayout = async (
  nodes: Node[],
  edges: Edge[],
): Promise<Node[]> => {
  const elkGraph = createElkGraph(nodes, edges);
  const layout = await elk.layout(elkGraph);
  return extractLayoutedNodes(layout, nodes);
};

const createElkGraph = (nodes: Node[], edges: Edge[]) => {
  return {
    id: "root",
    layoutOptions: {
      "elk.algorithm": "layered",
      "elk.direction": "DOWN",
    },
    ...buildElkNodes(
      nodes.filter((node) => !node.parentId),
      nodes,
      edges,
    ),
  };
};

const buildElkNodes = (
  nodes: Node[],
  allNodes: Node[],
  allEdges: Edge[],
): {
  children: ElkNode[];
  edges: ElkExtendedEdge[];
} => {
  return {
    children: nodes.map((node) => {
      const childNodes = node.data.expanded
        ? allNodes.filter((n) => n.parentId === node.id)
        : [];
      const padding = calculateNodePadding(childNodes.length);
      return {
        id: node.id,
        width: node.width,
        height: node.height,
        layoutOptions: createLayoutOptions(node, padding),
        ...buildElkNodes(childNodes, allNodes, allEdges),
      };
    }),
    edges: allEdges
      .filter((edge) => {
        const source = nodes.find((n) => n.id === edge.source);
        const target = nodes.find((n) => n.id === edge.target);
        return !!source && !!target;
      })
      .map((edge) => ({
        id: edge.id,
        sources: [edge.source],
        targets: [edge.target],
      })),
  };
};

const extractLayoutedNodes = (layout: ElkNode, flatNodes: Node[]): Node[] => {
  let layoutedNodes: Node[] = [];

  if (!layout.children) return layoutedNodes;

  layout.children.forEach((node) => {
    const originalNode = flatNodes.find((n) => n.id === node.id)!;
    layoutedNodes.push({
      ...originalNode,
      position: {
        x: node.x || 0,
        y: node.y || 0,
      },
      width: node.width || 0,
      height: node.height || 0,
    });
    layoutedNodes = layoutedNodes.concat(extractLayoutedNodes(node, flatNodes));
  });

  return layoutedNodes;
};

const calculateNodePadding = (childCount: number) => {
  const topPadding =
    headerHeight +
    nodePadding +
    nodeBorder +
    (childCount > 0 ? distanceBetweenHeaderAndContent + contentPadding : 0);
  const sidePadding = contentPadding + nodePadding + nodeBorder;

  return { top: topPadding, side: sidePadding };
};

const createLayoutOptions = (
  node: Node,
  padding: { top: number; side: number },
) => {
  return {
    "elk.algorithm": "layered",
    "elk.direction": node.type === "conditional" ? "RIGHT" : "DOWN",
    "elk.padding": `[top=${padding.top}, left=${padding.side}, bottom=${padding.side}, right=${padding.side}]`,
    "elk.spacing.nodeNode": `${distanceBetweenNodes}`,
    "elk.nodeSize.constraints": "MINIMUM_SIZE",
    "elk.layered.spacing.nodeNodeBetweenLayers": `${distanceBetweenNodes}`,
    "elk.nodeSize.minimum": `(${node.width},${node.height})`,
  };
};
