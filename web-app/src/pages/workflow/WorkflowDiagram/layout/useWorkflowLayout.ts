import {
  Edge,
  useEdgesState,
  useNodesState,
  useReactFlow,
} from "@xyflow/react";
import useWorkflow, {
  LayoutedNode,
  TaskConfigWithId,
} from "@/stores/useWorkflow";
import { useMemo, useEffect } from "react";
import { calculateNodesSize, getLayoutedElements } from "../layout";
import { buildWorkflowNodes } from "./nodeBuilder";

export const useWorkflowLayout = (tasks: TaskConfigWithId[]) => {
  const [reactFlowNodes, setReactFlowNodes, onNodesChange] =
    useNodesState<LayoutedNode>([]);

  const [reactFlowEdges, setReactFlowEdges, onEdgesChange] =
    useEdgesState<Edge>([]);
  const reactFlowInstance = useReactFlow();

  const { nodes, edges, setNodes, setEdges, layoutedNodes, setLayoutedNodes } =
    useWorkflow();

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
    const { nodes, edges } = buildWorkflowNodes(tasks);
    setNodes(nodes);
    setEdges(edges);
  }, [setEdges, setNodes, tasks]);

  useEffect(() => {
    const updateLayout = async () => {
      const nodesWithSize = calculateNodesSize(nodes);
      const layoutedNodes = await getLayoutedElements(nodesWithSize, edges);
      setLayoutedNodes(layoutedNodes);
    };
    updateLayout();
  }, [nodes, edges, setLayoutedNodes]);

  useEffect(() => {
    if (!reactFlowInstance) return;

    setReactFlowEdges(edges);
    setReactFlowNodes(layoutedNodes);
    reactFlowInstance.fitView(fitViewOptions);
  }, [
    reactFlowInstance,
    layoutedNodes,
    edges,
    fitViewOptions,
    setReactFlowEdges,
    setReactFlowNodes,
  ]);

  return {
    reactFlowNodes,
    reactFlowEdges,
    onNodesChange,
    onEdgesChange,
    fitViewOptions,
  };
};
