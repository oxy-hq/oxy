import {
  Node,
  Edge,
  TaskConfigWithId,
  TaskType,
  ConditionalTaskConfigWithId,
  NoneTaskNodeType,
  NodeType,
} from "@/stores/useWorkflow";

interface NodeBuilderResult {
  nodes: Node[];
  edges: Edge[];
}

const createNode = ({
  nodeId,
  task,
  index,
  metadata,
  parentId,
  name,
  type,
}: {
  nodeId: string;
  task: TaskConfigWithId;
  index: number;
  metadata?: Record<string, unknown>;
  parentId?: string;
  name?: string;
  type?: NodeType;
}) => {
  return {
    id: nodeId,
    data: {
      task: { ...task, id: nodeId },
      id: nodeId,
      index,
      metadata,
    },
    type: type ?? task.type,
    name: name ?? task.name,
    parentId,
    hidden: false,
    width: 0,
    height: 0,
    children: [],
  };
};

const createConditionalIfNode = (
  task: ConditionalTaskConfigWithId & { id: string },
  condition: { if: string },
  index: number,
  conditionIndex: number,
): Node => {
  const nodeId = `${task.id}-condition-${conditionIndex}`;
  return createNode({
    nodeId,
    task,
    index,
    metadata: {
      condition: condition.if,
    },
    type: NoneTaskNodeType.CONDITIONAL_IF,
    parentId: task.id,
    name: condition.if,
  });
};

const createConditionalElseNode = (
  task: ConditionalTaskConfigWithId & { id: string },
  index: number,
): Node => {
  const nodeId = `${task.id}-else`;
  return createNode({
    nodeId,
    task,
    index,
    parentId: task.id,
    type: NoneTaskNodeType.CONDITIONAL_ELSE,
    name: "Else",
  });
};

const buildConditionalNodes = (
  task: ConditionalTaskConfigWithId & { id: string },
  index: number,
  level: number,
): { nodes: Node[]; edges: Edge[] } => {
  const result: NodeBuilderResult = { nodes: [], edges: [] };

  task.conditions.forEach((condition, condIndex) => {
    const ifNode = createConditionalIfNode(task, condition, index, condIndex);
    result.nodes.push(ifNode);

    const { nodes, edges } = buildWorkflowNodes(
      condition.tasks,
      ifNode.id,
      level + 1,
    );
    result.nodes.push(...nodes);
    result.edges.push(...edges);

    if (condIndex > 0) {
      result.edges.push({
        id: `${task.id}-condition-${index - 1}-${task.id}-condition-${condIndex}`,
        source: `${task.id}-condition-${index - 1}`,
        target: ifNode.id,
        hidden: true,
      });
    }
  });

  if (task.else) {
    const elseNode = createConditionalElseNode(task, index);
    result.nodes.push(elseNode);

    const { nodes, edges } = buildWorkflowNodes(
      task.else,
      elseNode.id,
      level + 1,
    );
    result.nodes.push(...nodes);
    result.edges.push(...edges);

    if (task.conditions.length > 0) {
      result.edges.push({
        id: `${task.id}-condition-${task.conditions.length - 1}-${task.id}-else`,
        source: `${task.id}-condition-${task.conditions.length - 1}`,
        target: elseNode.id,
        hidden: true,
      });
    }
  }

  return {
    nodes: result.nodes,
    edges: result.edges,
  };
};

export const buildWorkflowNodes = (
  tasks: TaskConfigWithId[],
  parentId?: string,
  level = 0,
): NodeBuilderResult => {
  const result: NodeBuilderResult = { nodes: [], edges: [] };

  tasks.forEach((task, index) => {
    const node = createNode({
      nodeId: task.id,
      index,
      task,
      parentId,
    });

    if (task.type === TaskType.LOOP_SEQUENTIAL) {
      const { nodes, edges } = buildWorkflowNodes(
        task.tasks,
        node.id,
        level + 1,
      );
      result.nodes.push(...nodes);
      result.edges.push(...edges);
    } else if (task.type === TaskType.CONDITIONAL) {
      const { nodes, edges } = buildConditionalNodes(task, index, level + 1);
      result.nodes.push(...nodes);
      result.edges.push(...edges);
    }

    result.nodes.push(node);

    if (index > 0) {
      const prevId = tasks[index - 1].id;
      result.edges.push({
        id: `${prevId}-${task.id}`,
        source: prevId,
        target: task.id,
      });
    }
  });

  return {
    nodes: result.nodes,
    edges: sortEdges(result.edges),
  };
};

const sortEdges = (edges: Edge[]): Edge[] =>
  edges.sort((a, b) => a.id.length - b.id.length);
