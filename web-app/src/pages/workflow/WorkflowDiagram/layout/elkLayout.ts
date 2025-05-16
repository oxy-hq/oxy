import ELK, { ElkNode } from "elkjs/lib/elk.bundled.js";
import { Node, Edge, LayoutedNode } from "@/stores/useWorkflow";
import {
  contentPadding,
  distanceBetweenHeaderAndContent,
  distanceBetweenNodes,
  headerHeight,
  nodeBorder,
  nodePadding,
} from "./constants";

const elk = new ELK();

export const createElkLayout = async (
  nodes: Node[],
  edges: Edge[],
): Promise<LayoutedNode[]> => {
  const flatNodes: Node[] = [];
  const elkGraph = createElkGraph(nodes, edges, flatNodes);
  const layout = await elk.layout(elkGraph);
  return extractLayoutedNodes(layout, flatNodes);
};

const createElkGraph = (nodes: Node[], edges: Edge[], flatNodes: Node[]) => {
  const children = buildElkNodes(
    nodes.filter((n) => !n.parentId && !n.hidden),
    nodes,
    flatNodes,
  );

  const visibleEdges = edges.filter((edge) => {
    const source = nodes.find((n) => n.id === edge.source);
    const target = nodes.find((n) => n.id === edge.target);
    return source && target && !source.hidden && !target.hidden;
  });

  return {
    id: "root",
    layoutOptions: {
      "elk.algorithm": "layered",
      "elk.direction": "DOWN",
    },
    children,
    edges: visibleEdges.map((edge) => ({
      id: edge.id,
      sources: [edge.source],
      targets: [edge.target],
    })),
  };
};

const buildElkNodes = (
  nodes: Node[],
  allNodes: Node[],
  flatNodes: Node[],
): ElkNode[] => {
  return nodes.map((node) => {
    flatNodes.push(node);
    const childNodes = allNodes.filter(
      (n) => n.parentId === node.id && !n.hidden,
    );

    const padding = calculateNodePadding(childNodes.length);

    return {
      id: node.id,
      width: node.width,
      height: node.height,
      layoutOptions: createLayoutOptions(node, padding),
      children: buildElkNodes(childNodes, allNodes, flatNodes),
      parentId: node.parentId,
    };
  });
};

const extractLayoutedNodes = (
  layout: ElkNode,
  flatNodes: Node[],
): LayoutedNode[] => {
  let layoutedNodes: LayoutedNode[] = [];

  if (!layout.children) return layoutedNodes;

  layout.children.forEach((node) => {
    const originalNode = flatNodes.find((n) => n.id === node.id)!;
    layoutedNodes.push({
      ...originalNode,
      position: { x: node.x || 0, y: node.y || 0 },
      size: { width: node.width || 0, height: node.height || 0 },
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
