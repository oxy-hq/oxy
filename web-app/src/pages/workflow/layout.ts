import ELK, { ElkNode } from "elkjs/lib/elk.bundled.js";

import {
  ConditionalTaskConfigWithId,
  Edge,
  LayoutedNode,
  Node,
  NoneTaskNodeType,
  TaskConfigWithId,
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

const elk = new ELK();

// calculate the size of a node that contains a list of tasks
// the size of the node is the max width of its children + padding
// the height of the node is the sum of its children's height + padding
const computeTasksContainerNodeSize = (
  node: Node,
  nodesWithSize: Node[],
): { width: number; height: number } => {
  const children = nodesWithSize
    .filter((n) => n.parentId === node.id)
    .filter((n) => !n.hidden);

  let totalHeight = 0;
  let maxWidth = minNodeWidth;

  children.forEach((child, index) => {
    if (child.size.width === 0) computeSpecialNodeSize(child, nodesWithSize);
    maxWidth = Math.max(maxWidth, child.size.width);
    totalHeight += child.size.height + (index > 0 ? distanceBetweenNodes : 0);
  });

  // Set all children to be the max width
  children.forEach((child) => {
    child.size.width = maxWidth;
    child.width = maxWidth;
  });

  let width = maxWidth;
  if (children.length > 0) {
    width += 2 * contentPadding + 2 * nodePadding + 2 * nodeBorder;
  }

  let height = totalHeight + headerHeight + paddingHeight + nodeBorderHeight;
  if (children.length > 0) {
    height += distanceBetweenHeaderAndContent + contentPaddingHeight;
  }

  return { width, height };
};

// calculate the size of a conditional node
// the size of the node is the max width of its children + padding
// the height of the node is the sum of its children's height + padding
const computeConditionalNodeSize = (
  node: Node,
  nodesWithSize: Node[],
): { width: number; height: number } => {
  const children = nodesWithSize
    .filter((n) => n.parentId === node.id && n.data.index == 0)
    .filter((n) => !n.hidden);

  let totalWidth = 0;
  let maxHeight = 0;

  children.forEach((child, index) => {
    if (child.size.width === 0) computeSpecialNodeSize(child, nodesWithSize);
    maxHeight = Math.max(maxHeight, child.size.height);
    totalWidth += child.size.width + (index > 0 ? distanceBetweenNodes : 0);
  });

  let height = maxHeight + headerHeight + paddingHeight + nodeBorderHeight;
  if (children.length > 0) {
    height += distanceBetweenHeaderAndContent + contentPaddingHeight;
  }

  let width = totalWidth;
  if (children.length > 0) {
    width += 2 * contentPadding + 2 * nodePadding + 2 * nodeBorder;
  }

  return { width, height };
};

function computeSpecialNodeSize(node: Node, nodeWithSize: Node[]) {
  switch (node.type) {
    case NoneTaskNodeType.CONDITIONAL_ELSE:
    case NoneTaskNodeType.CONDITIONAL_IF:
    case TaskType.LOOP_SEQUENTIAL: {
      const { width, height } = computeTasksContainerNodeSize(
        node,
        nodeWithSize,
      );
      node.size = { width, height };
      node.width = width;
      node.height = height;
      break;
    }

    case TaskType.CONDITIONAL: {
      const { width, height } = computeConditionalNodeSize(node, nodeWithSize);
      node.size = { width, height };
      node.width = width;
      node.height = height;
      break;
    }
  }
}

export function calculateNodesSize(nodes: Node[]): Node[] {
  const nodesWithSize = [...nodes.map((n) => ({ ...n }))];
  nodesWithSize.forEach((node) => {
    if (
      node.type !== TaskType.LOOP_SEQUENTIAL &&
      node.type !== TaskType.CONDITIONAL &&
      node.type !== NoneTaskNodeType.CONDITIONAL_ELSE &&
      node.type !== NoneTaskNodeType.CONDITIONAL_IF
    ) {
      const width = smallestNodeWidth;
      const height = normalNodeHeight;
      node.size = { width, height };
      node.width = width;
      node.height = height;
    }
  });

  nodesWithSize.forEach((node) => computeSpecialNodeSize(node, nodesWithSize));
  const maxWidth = nodesWithSize.reduce(
    (max, node) => Math.max(max, node.size.width),
    0,
  );
  nodesWithSize
    .filter((n) => n.parentId === undefined)
    .forEach((node) => {
      node.size.width = maxWidth;
      node.width = maxWidth;
    });

  return nodesWithSize;
}

export const getLayoutedElements = async (nodes: Node[], edges: Edge[]) => {
  const flatNodes: Node[] = [];
  const buildChildren = (ns: Node[]): ElkNode[] => {
    if (!ns) return [];
    const layoutedNodes = ns.map((node) => {
      flatNodes.push(node);
      const childNodes = nodes.filter(
        (n) => n.parentId === node.id && !n.hidden,
      );
      let topPadding = headerHeight + nodePadding + nodeBorder;
      const padding = contentPadding + nodePadding + nodeBorder;
      if (childNodes.length > 0) {
        topPadding += distanceBetweenHeaderAndContent + contentPadding;
      }

      const elkNode = {
        id: node.id,
        width: node.size.width,
        height: node.size.height,
        layoutOptions: {
          "elk.algorithm": "layered",
          "elk.direction": node.type == "conditional" ? "RIGHT" : "DOWN",
          "elk.padding": `[top=${topPadding}, left=${padding}, bottom=${padding}, right=${padding}]`,
          "elk.spacing.nodeNode": `${distanceBetweenNodes}`,
          "elk.nodeSize.constraints": "MINIMUM_SIZE",
          "elk.layered.spacing.nodeNodeBetweenLayers": `${distanceBetweenNodes}`,
          "elk.nodeSize.minimum": `(${node.size.width},${node.size.height})`,
        },
        children: buildChildren(childNodes),
        parentId: node.parentId,
      };
      return elkNode;
    });
    return layoutedNodes;
  };

  const children = buildChildren(
    nodes.filter((n) => n.parentId === undefined && !n.hidden),
  );
  const visibleEdges = edges.filter((edge) => {
    const source = nodes.find((n) => n.id === edge.source);
    const target = nodes.find((n) => n.id === edge.target);
    return source && target && !source.hidden && !target.hidden;
  });
  const graph = {
    id: "root",
    layoutOptions: {
      "elk.algorithm": "layered",
      "elk.direction": "DOWN",
    },
    children: children,
    edges: visibleEdges.map((edge) => ({
      id: edge.id,
      sources: [edge.source],
      targets: [edge.target],
    })),
  };
  const layout = await elk.layout(graph);

  // flatten the layout nodes so we can pass it inside reactflow.
  const getFlatNodes = (layout: ElkNode) => {
    let nodes: LayoutedNode[] = [];
    if (!layout.children) return nodes;
    layout.children.map((node) => {
      const realNode = flatNodes.find((n) => n.id === node.id)!;
      nodes.push({
        ...realNode,
        position: { x: node.x || 0, y: node.y || 0 },
      });
      nodes = nodes.concat(getFlatNodes(node));
    });
    return nodes;
  };
  return getFlatNodes(layout);
};

export const buildNodes = (
  tasks: TaskConfigWithId[],
  parentId: string | undefined = undefined,
  level = 0,
) => {
  let edges: Edge[] = [];
  let nodes: Node[] = [];
  tasks.map((task, index) => {
    const id = task.id;

    const node: Node = {
      id,
      data: {
        task: { ...task, id: id },
        id,
        index,
        canMoveDown: index < tasks.length - 1,
        canMoveUp: index > 0,
      },
      type: task.type,
      parentId,
      name: task.name,
      size: {
        width: 0,
        height: 0,
      },
      hidden: false,
      width: 0,
      height: 0,
      children: [],
    };
    if (task.type === TaskType.LOOP_SEQUENTIAL) {
      const { nodes: loopNodes, edges: loopEdges } = buildNodes(
        task.tasks,
        id,
        level + 1,
      );
      nodes = nodes.concat(loopNodes);
      edges = edges.concat(loopEdges);
    } else if (task.type === TaskType.CONDITIONAL) {
      const { nodes: conditionNodes, edges: conditionEdges } =
        buildConditionalNodes(task, index, level + 1);
      nodes = nodes.concat(conditionNodes);
      edges = edges.concat(conditionEdges);
    }
    nodes.push(node);
    if (index > 0) {
      const prevId = tasks[index - 1].id;
      edges.push({
        id: `${prevId}-${id}`,
        source: prevId,
        target: id,
      });
    }
  });
  edges = edges.sort((a, b) => {
    return a.id.length - b.id.length;
  });
  return { nodes, edges };
};

// Goal:
// 1. Create a node that contains tasks for else: if else is not empty
// 2. Create a node that contains tasks for each condition: if condition is not empty
// 3. The order of the nodes should be condition 1, condition 2, ..., condition n, else
const buildConditionalNodes = (
  task: ConditionalTaskConfigWithId & { id: string },
  index: number,
  level: number,
): {
  nodes: Node[];
  edges: Edge[];
} => {
  let edges: Edge[] = [];
  let nodes: Node[] = [];
  const id = task.id;
  task.conditions.forEach((cd, ci) => {
    const ifNode: Node = {
      id: `${id}-condition-${ci}`,
      data: {
        task: { ...task, id: `${id}-condition-${ci}` },
        id: `${id}-condition-${ci}`,
        index,
        canMoveDown: false,
        canMoveUp: false,
        metadata: {
          condition: cd.if,
        },
      },
      type: NoneTaskNodeType.CONDITIONAL_IF,
      parentId: id,
      name: cd.if,
      size: {
        width: 0,
        height: 0,
      },
      hidden: false,
      width: 0,
      height: 0,
      children: [],
    };
    nodes.push(ifNode);
    const { nodes: conditionNodes, edges: conditionEdges } = buildNodes(
      cd.tasks,
      ifNode.id,
      level + 1,
    );
    nodes = nodes.concat(conditionNodes);
    edges = edges.concat(conditionEdges);

    // add a hidden edge from the previous condition to this one
    // this help the layout engine to layout the nodes position correctly
    if (ci > 0) {
      edges.push({
        id: `${id}-condition-${ci - 1}-${id}-condition-${ci}`,
        source: `${id}-condition-${ci - 1}`,
        target: ifNode.id,
        hidden: true,
      });
    }
  });
  if (task.else) {
    const elseNode: Node = {
      id: `${id}-else`,
      data: {
        task: { ...task, id: `${id}-else` },
        id: `${id}-else`,
        index,
        canMoveDown: false,
        canMoveUp: false,
      },
      type: NoneTaskNodeType.CONDITIONAL_ELSE,
      parentId: id,
      name: "Else",
      size: {
        width: 0,
        height: 0,
      },
      hidden: false,
      width: 0,
      height: 0,
      children: [],
    };
    nodes.push(elseNode);
    const { nodes: elseNodes, edges: elseEdges } = buildNodes(
      task.else,
      elseNode.id,
      level + 1,
    );
    nodes = nodes.concat(elseNodes);
    edges = edges.concat(elseEdges);

    // add a hidden edge from the previous condition to this one
    // this help the layout engine to layout the nodes position correctly
    if (task.conditions.length > 0) {
      edges.push({
        id: `${id}-condition-${task.conditions.length - 1}-${id}-else`,
        source: `${id}-condition-${task.conditions.length - 1}`,
        target: elseNode.id,
        hidden: true,
      });
    }
  }
  return { nodes, edges };
};
