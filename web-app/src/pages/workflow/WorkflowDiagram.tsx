import { useEffect, useMemo, useRef } from "react";

import {
  Background,
  BackgroundVariant,
  Controls,
  ReactFlow,
  useEdgesState,
  useNodesState,
  useReactFlow,
} from "@xyflow/react";

import useWorkflow, {
  Edge,
  LayoutedNode,
  NodeType,
  TaskConfigWithId,
} from "@/stores/useWorkflow";

import { DiagramNode } from "./DiagramNode";
import { buildNodes, calculateNodesSize, getLayoutedElements } from "./layout";

const nodeTypes: Record<NodeType, typeof DiagramNode> = {
  execute_sql: DiagramNode,
  loop_sequential: DiagramNode,
  formatter: DiagramNode,
  agent: DiagramNode,
  workflow: DiagramNode,
  conditional: DiagramNode,
  "conditional-else": DiagramNode,
  "conditional-if": DiagramNode,
} as const;

const WorkflowDiagram = ({ tasks }: { tasks: TaskConfigWithId[] }) => {
  const [reactFlowNodes, setReactFlowNodes, onNodesChange] =
    useNodesState<LayoutedNode>([]);
  const [reactFlowEdges, setReactFlowEdges, onEdgesChange] =
    useEdgesState<Edge>([]);
  const setNodes = useWorkflow((state) => state.setNodes);
  const setEdges = useWorkflow((state) => state.setEdges);
  const setLayoutedNodes = useWorkflow((state) => state.setLayoutedNodes);
  const layoutedNodes = useWorkflow((state) => state.layoutedNodes);
  const reactFlowInstance = useReactFlow();
  useEffect(() => {
    const { nodes, edges } = buildNodes(tasks);
    setNodes(nodes);
    setEdges(edges);
  }, [tasks, setNodes, setEdges]);
  const nodes = useWorkflow((state) => state.nodes);
  const edges = useWorkflow((state) => state.edges);
  const reactFlowWrapper = useRef(null);

  useEffect(() => {
    const getLayout = async () => {
      const nodesWithSize = calculateNodesSize(nodes);
      const lnodes = await getLayoutedElements(nodesWithSize, [...edges]);
      setLayoutedNodes(lnodes);
    };
    getLayout();
  }, [nodes, edges, setLayoutedNodes]);

  const fitViewOptions = useMemo(
    () => ({
      maxZoom: 1,
      minZoom: 0.1,
      nodes: layoutedNodes,
      duration: 0,
    }),
    [layoutedNodes],
  );

  useEffect(() => {
    if (reactFlowInstance) {
      setReactFlowEdges(edges);
      setReactFlowNodes(layoutedNodes);
      reactFlowInstance.fitView(fitViewOptions);
    }
  }, [
    reactFlowInstance,
    layoutedNodes,
    edges,
    setReactFlowEdges,
    setReactFlowNodes,
    fitViewOptions,
  ]);
  return (
    <div className="w-full h-full" ref={reactFlowWrapper}>
      <ReactFlow
        nodeTypes={nodeTypes}
        proOptions={{ hideAttribution: true }}
        onNodesChange={onNodesChange}
        onEdgesChange={onEdgesChange}
        nodes={reactFlowNodes}
        edges={reactFlowEdges}
        fitView
        draggable={false}
        nodesDraggable={false}
      >
        <Controls showInteractive={false} fitViewOptions={fitViewOptions} />
        <Background color="#ccc" variant={BackgroundVariant.Dots} />
      </ReactFlow>
    </div>
  );
};
export default WorkflowDiagram;
