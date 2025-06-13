import {
  Node,
  NodeType,
  NoneTaskNodeType,
  TaskType,
} from "@/stores/useWorkflow";
import {
  contentPadding,
  contentPaddingHeight,
  distanceBetweenHeaderAndContent,
  distanceBetweenNodes,
  headerHeight,
  minNodeWidth,
  nodeBorder,
  nodeBorderHeight,
  nodePadding,
  normalNodeHeight,
  paddingHeight,
  smallestNodeWidth,
} from "./constants";

export const computeNodeDimensions = (node: Node, allNodes: Node[]): void => {
  if (isSpecialNode(node.type)) {
    computeSpecialNodeSize(node, allNodes);
  } else {
    computeBasicNodeSize(node);
  }
};

const isSpecialNode = (type: NodeType): boolean => {
  return [
    NoneTaskNodeType.CONDITIONAL_ELSE,
    NoneTaskNodeType.CONDITIONAL_IF,
    TaskType.LOOP_SEQUENTIAL,
    TaskType.CONDITIONAL,
  ].includes(type);
};

const computeBasicNodeSize = (node: Node): void => {
  node.width = smallestNodeWidth;
  node.height = normalNodeHeight;
};

const computeSpecialNodeSize = (node: Node, allNodes: Node[]): void => {
  switch (node.type) {
    case NoneTaskNodeType.CONDITIONAL_ELSE:
    case NoneTaskNodeType.CONDITIONAL_IF:
    case TaskType.LOOP_SEQUENTIAL: {
      const verticalLayout = computeVerticalContainerSize(node, allNodes);
      node.width = verticalLayout.width;
      node.height = verticalLayout.height;
      break;
    }
    case TaskType.CONDITIONAL: {
      const horizontalLayout = computeHorizontalContainerSize(node, allNodes);
      node.width = horizontalLayout.width;
      node.height = horizontalLayout.height;
      break;
    }
  }
};

const computeVerticalContainerSize = (
  node: Node,
  allNodes: Node[],
): { width: number; height: number } => {
  const children = getVisibleChildren(node, allNodes);

  let totalHeight = 0;
  let maxWidth = minNodeWidth;

  children.forEach((child, index) => {
    if (child.width === 0) computeNodeDimensions(child, allNodes);
    maxWidth = Math.max(maxWidth, child.width);
    totalHeight += child.height + (index > 0 ? distanceBetweenNodes : 0);
  });

  children.forEach((child) => {
    child.width = maxWidth;
  });

  return calculateContainerDimensions(maxWidth, totalHeight, children.length);
};

const computeHorizontalContainerSize = (
  node: Node,
  allNodes: Node[],
): { width: number; height: number } => {
  const children = getVisibleChildren(node, allNodes);

  let totalWidth = 0;
  let maxHeight = 0;

  children.forEach((child, index) => {
    if (child.width === 0) computeNodeDimensions(child, allNodes);
    maxHeight = Math.max(maxHeight, child.height);
    totalWidth += child.width + (index > 0 ? distanceBetweenNodes : 0);
  });

  return calculateContainerDimensions(totalWidth, maxHeight, children.length);
};

const getVisibleChildren = (node: Node, allNodes: Node[]): Node[] => {
  return allNodes
    .filter((n) => n.parentId === node.id)
    .filter((n) => !n.hidden);
};

const calculateContainerDimensions = (
  baseWidth: number,
  baseHeight: number,
  childCount: number,
): { width: number; height: number } => {
  let width = baseWidth;
  let height = baseHeight;

  height += headerHeight + paddingHeight + nodeBorderHeight;

  if (childCount > 0) {
    width += 2 * (contentPadding + nodePadding + nodeBorder);
    height += distanceBetweenHeaderAndContent + contentPaddingHeight;
  }

  return { width, height };
};
