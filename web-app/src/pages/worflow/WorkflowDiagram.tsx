import { useEffect, useRef } from "react";

import {
  Background,
  BackgroundVariant,
  ReactFlow,
  useReactFlow,
} from "@xyflow/react";
import ELK, { ElkNode } from "elkjs/lib/elk.bundled.js";

import useDiagram, { Edge, LayoutedNode, Node } from "@/stores/useDiagram";

import { ConnectionLine } from "./ConnectionLine";
import {
  contentPadding,
  contentPaddingHeight,
  distanceBetweenHeaderAndContent,
  distanceBetweenNodes,
  headerHeight,
  nodePadding,
  normalNodeHeight,
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
    let maxWidth = 200;
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
      width += 2 * contentPadding;
    }
    let height = totalHeight + normalNodeHeight;
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

  return newNodes.sort((a, b) => a.id.length - b.id.length);
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
      let topPadding = headerHeight + nodePadding;
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
          "elk.padding": `[top=${topPadding}, left=${contentPadding}, bottom=${contentPadding + nodePadding}, right=${contentPadding}]`,
          "elk.spacing.nodeNode": `${distanceBetweenNodes}`,
          "elk.layered.spacing.nodeNodeBetweenLayers": `${distanceBetweenNodes}`,
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
  steps: StepData[],
  parentId: string | undefined = undefined,
  level = 0,
) => {
  let idPrefix = null;
  if (parentId) {
    idPrefix = parentId;
  }
  let edges: Edge[] = [];
  let nodes: Node[] = [];
  steps.map((step, index) => {
    let id;
    if (idPrefix) {
      id = `${idPrefix}-${index}`;
    } else {
      id = index.toString();
    }

    // else {
    const node: Node = {
      id,
      data: { step: { ...step, id: id }, id },
      type: step.type,
      parentId,
      name: step.name,
      size: {
        width: 0,
        height: 0,
      },
      hidden: false,
      width: 0,
      height: 0,
      children: [],
    };
    if (step.type === "loop_sequential") {
      const { nodes: loopNodes, edges: loopEdges } = buildNodes(
        step.steps!,
        id,
        level + 1,
      );
      nodes = nodes.concat(loopNodes);
      edges = edges.concat(loopEdges);
    }
    nodes.push(node);
    if (index > 0) {
      let prevId;
      if (idPrefix) {
        prevId = `${idPrefix}-${index - 1}`;
      } else {
        prevId = (index - 1).toString();
      }
      edges.push({
        id: `${prevId}-${id}`,
        source: prevId,
        target: id,
      });
      // }
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
};

export type StepData = {
  id: string;
  name: string;
  type: string;
  steps?: StepData[];
};

const WorkflowDiagram = ({ steps }: { steps: StepData[] }) => {
  const setNodes = useDiagram((state) => state.setNodes);
  const setEdges = useDiagram((state) => state.setEdges);
  const setLayoutedNodes = useDiagram((state) => state.setLayoutedNodes);
  const layoutedNodes = useDiagram((state) => state.layoutedNodes);
  useEffect(() => {
    const { nodes, edges } = buildNodes(steps);
    setNodes(nodes);
    setEdges(edges);
  }, [steps]);
  const nodes = useDiagram((state) => state.nodes);
  const edges = useDiagram((state) => state.edges);
  useEffect(() => {
    const getLayout = async () => {
      const nodesWithSize = calculateNodesSize(nodes);
      const lnodes = await getLayoutedElements(nodesWithSize, [...edges]);
      setLayoutedNodes(lnodes);
    };
    getLayout();
  }, [nodes, edges]);
  const reactFlowInstance = useReactFlow(); // Access React Flow instance
  const reactFlowWrapper = useRef(null);

  useEffect(() => {
    if (reactFlowInstance) {
      reactFlowInstance.fitView(); // Automatically centers and fits the graph
    }
  }, [reactFlowInstance]);
  return (
    <div style={{ width: "100%", height: "100%" }} ref={reactFlowWrapper}>
      <ReactFlow
        nodes={layoutedNodes}
        edges={edges}
        nodeTypes={nodeTypes}
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
