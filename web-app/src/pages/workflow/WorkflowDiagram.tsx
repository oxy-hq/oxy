import { useEffect, useRef } from "react";

import {
  Background,
  BackgroundVariant,
  ReactFlow,
  useReactFlow,
} from "@xyflow/react";
import ELK, { ElkNode } from "elkjs/lib/elk.bundled.js";

import useWorkflow, {
  Edge,
  LayoutedNode,
  Node,
  TaskConfig,
  TaskType,
} from "@/stores/useWorkflow";

import { ConnectionLine } from "./ConnectionLine";
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
import { StepNode } from "./StepNode";

const elk = new ELK();

function calculateNodesSize(nodes: Node[]): Node[] {
  const newNodes = [...nodes.map((n) => ({ ...n }))];
  newNodes.forEach((node) => {
    if (node.type !== "loop_sequential") {
      const width = smallestNodeWidth;
      const height = normalNodeHeight;
      node.size = { width, height };
      node.width = width;
      node.height = height;
    }
  });

  function computeSize(node: Node) {
    if (node.type !== "loop_sequential") return;
    const children = newNodes
      .filter((n) => n.parentId === node.id)
      .filter((n) => !n.hidden);
    let totalHeight = 0;
    let maxWidth = minNodeWidth;
    children.forEach((child, index) => {
      if (child.size.width === 0) computeSize(child);
      maxWidth = Math.max(maxWidth, child.size.width);
      totalHeight += child.size.height + (index > 0 ? distanceBetweenNodes : 0);
    });

    // set all children to be the max width
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

    node.size = {
      width,
      height,
    };
    node.width = width;
    node.height = height;
  }

  newNodes.forEach((node) => computeSize(node));
  const maxWidth = newNodes.reduce(
    (max, node) => Math.max(max, node.size.width),
    0,
  );
  newNodes
    .filter((n) => n.parentId === undefined)
    .forEach((node) => {
      node.size.width = maxWidth;
      node.width = maxWidth;
    });

  return newNodes;
}

const getLayoutedElements = async (nodes: Node[], edges: Edge[]) => {
  const flatNodes: Node[] = [];
  const buildChildren = (ns: Node[]): ElkNode[] => {
    if (!ns) return [];
    return ns.map((node) => {
      flatNodes.push(node);
      const childNodes = nodes.filter(
        (n) => n.parentId === node.id && !n.hidden,
      );
      let topPadding = headerHeight + nodePadding + nodeBorder;
      const padding = contentPadding + nodePadding + nodeBorder;
      if (childNodes.length > 0) {
        topPadding += distanceBetweenHeaderAndContent + contentPadding;
      }
      return {
        id: node.id,
        width: node.size.width,
        height: node.size.height,
        layoutOptions: {
          "elk.algorithm": "layered",
          "elk.direction": "DOWN",
          "elk.padding": `[top=${topPadding}, left=${padding}, bottom=${padding}, right=${padding}]`,
          "elk.spacing.nodeNode": distanceBetweenNodes,
          "elk.layered.spacing.nodeNodeBetweenLayers": distanceBetweenNodes,
        },
        children: buildChildren(childNodes),
        parentId: node.parentId,
      };
    });
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

const buildNodes = (
  tasks: TaskConfig[],
  parentId: string | undefined = undefined,
  level = 0,
) => {
  let edges: Edge[] = [];
  let nodes: Node[] = [];
  tasks.map((task, index) => {
    const id = task.id;

    // else {
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
    };
    if (task.type === TaskType.LOOP_SEQUENTIAL) {
      const { nodes: loopNodes, edges: loopEdges } = buildNodes(
        task.tasks,
        id,
        level + 1,
      );
      nodes = nodes.concat(loopNodes);
      edges = edges.concat(loopEdges);
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

const nodeTypes = {
  execute_sql: StepNode,
  loop_sequential: StepNode,
  formatter: StepNode,
  agent: StepNode,
  workflow: StepNode,
};

const WorkflowDiagram = ({ tasks }: { tasks: TaskConfig[] }) => {
  const setNodes = useWorkflow((state) => state.setNodes);
  const setEdges = useWorkflow((state) => state.setEdges);
  const setLayoutedNodes = useWorkflow((state) => state.setLayoutedNodes);
  const layoutedNodes = useWorkflow((state) => state.layoutedNodes);
  useEffect(() => {
    const { nodes, edges } = buildNodes(tasks);
    setNodes(nodes);
    setEdges(edges);
  }, [tasks, setNodes, setEdges]);
  const nodes = useWorkflow((state) => state.nodes);
  const edges = useWorkflow((state) => state.edges);
  useEffect(() => {
    const getLayout = async () => {
      const nodesWithSize = calculateNodesSize(nodes);
      const lnodes = await getLayoutedElements(nodesWithSize, [...edges]);
      setLayoutedNodes(lnodes);
    };
    getLayout();
  }, [nodes, edges, setLayoutedNodes]);
  const reactFlowInstance = useReactFlow(); // Access React Flow instance
  const reactFlowWrapper = useRef(null);

  useEffect(() => {
    if (reactFlowInstance) {
      reactFlowInstance.fitView(); // Automatically centers and fits the graph
    }
  }, [reactFlowInstance]);
  return (
    <div className="w-full h-full" ref={reactFlowWrapper}>
      <ReactFlow
        nodes={layoutedNodes}
        edges={edges}
        nodeTypes={nodeTypes}
        proOptions={{ hideAttribution: true }}
        connectionLineComponent={ConnectionLine}
        connectionLineContainerStyle={{
          backgroundColor: "#D4D4D4",
        }}
        fitView
      >
        <Background color="#ccc" variant={BackgroundVariant.Dots} />
      </ReactFlow>
    </div>
  );
};
export default WorkflowDiagram;
