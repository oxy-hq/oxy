import type { Edge, Node } from "@xyflow/react";
import { useCallback, useEffect, useMemo, useState } from "react";
import type {
  ContextGraphNode as ContextGraphNodeType,
  ContextGraph as ContextGraphType
} from "@/types/contextGraph";
import { FOCUS_OPTIONS, type FocusType } from "./constants";

const VALID_FOCUS_TYPES = new Set<string>(FOCUS_OPTIONS.map((o) => o.value));

interface UseGraphFocusParams {
  data: ContextGraphType;
  setNodes: React.Dispatch<React.SetStateAction<Node[]>>;
  setEdges: React.Dispatch<React.SetStateAction<Edge[]>>;
}

interface UseGraphFocusReturn {
  focusedNodeId: string | null;
  selectedNode: ContextGraphNodeType | null;
  focusType: FocusType;
  expandAll: boolean;
  changeFocusType: (type: FocusType) => void;
  setExpandAll: (expand: boolean) => void;
  handleNodeClick: (event: React.MouseEvent, node: { id: string }) => void;
  handlePaneClick: () => void;
  resetView: () => void;
}

export function buildNeighbors(
  edges: Array<{ source: string; target: string }>
): Map<string, Set<string>> {
  const map = new Map<string, Set<string>>();
  for (const edge of edges) {
    if (!map.has(edge.source)) map.set(edge.source, new Set());
    if (!map.has(edge.target)) map.set(edge.target, new Set());
    map.get(edge.source)?.add(edge.target);
    map.get(edge.target)?.add(edge.source);
  }
  return map;
}

export function getConnectedNodes(
  neighbors: Map<string, Set<string>>,
  startIds: string[],
  maxDepth?: number
): Set<string> {
  const visited = new Set<string>();
  const queue: Array<{ id: string; depth: number }> = startIds.map((id) => ({
    id,
    depth: 0
  }));
  while (queue.length > 0) {
    const current = queue.shift()!;
    if (visited.has(current.id)) continue;
    visited.add(current.id);
    if (maxDepth !== undefined && current.depth >= maxDepth) continue;
    const adjacent = neighbors.get(current.id);
    if (adjacent) {
      for (const n of adjacent) {
        if (!visited.has(n)) queue.push({ id: n, depth: current.depth + 1 });
      }
    }
  }
  return visited;
}

export function computeFocusTypeVisible(
  focusType: FocusType,
  nodes: Array<{ id: string; type: string }>,
  neighbors: Map<string, Set<string>>
): Set<string> | null {
  if (focusType === "auto") return null;
  const seeds = nodes.filter((n) => n.type === focusType).map((n) => n.id);
  return getConnectedNodes(neighbors, seeds);
}

export function useGraphFocus({
  data,
  setNodes,
  setEdges
}: UseGraphFocusParams): UseGraphFocusReturn {
  const [focusedNodeId, setFocusedNodeId] = useState<string | null>(null);
  const [selectedNode, setSelectedNode] = useState<ContextGraphNodeType | null>(null);
  const [focusType, setFocusType] = useState<FocusType>(() => {
    const saved = localStorage.getItem("context-graph-focus-type");
    return saved && VALID_FOCUS_TYPES.has(saved) ? (saved as FocusType) : "auto";
  });
  const [expandAll, setExpandAll] = useState<boolean>(() => {
    const saved = localStorage.getItem("context-graph-expand-all");
    return saved === "true";
  });

  useEffect(() => {
    localStorage.setItem("context-graph-focus-type", focusType);
  }, [focusType]);

  useEffect(() => {
    localStorage.setItem("context-graph-expand-all", expandAll.toString());
  }, [expandAll]);

  const neighbors = useMemo(() => buildNeighbors(data.edges), [data.edges]);

  const handleNodeClick = useCallback(
    (_event: React.MouseEvent, node: { id: string }) => {
      setFocusedNodeId((prev) => (prev === node.id ? null : node.id));
      const contextNode = data.nodes.find((n) => n.id === node.id);
      setSelectedNode((prev) => (prev?.id === node.id ? null : (contextNode ?? null)));
    },
    [data.nodes]
  );

  const handlePaneClick = useCallback(() => {
    setFocusedNodeId(null);
    setSelectedNode(null);
  }, []);

  const changeFocusType = useCallback((type: FocusType) => {
    setFocusType(type);
    setFocusedNodeId(null);
    setSelectedNode(null);
  }, []);

  const resetView = useCallback(() => {
    setFocusedNodeId(null);
    setSelectedNode(null);
  }, []);

  const focusTypeVisible = useMemo(
    () => computeFocusTypeVisible(focusType, data.nodes, neighbors),
    [focusType, data.nodes, neighbors]
  );

  useEffect(() => {
    if (!focusedNodeId) {
      setNodes((nds) =>
        nds.map((node) => {
          let opacity = 1;
          if (focusTypeVisible) {
            const isFocusedType = (node.data as { type: string }).type === focusType;
            const isConnected = focusTypeVisible.has(node.id);
            if (!isFocusedType) {
              opacity = isConnected ? 0.5 : 0;
            }
          }
          return {
            ...node,
            data: {
              ...node.data,
              opacity,
              showLeftHandle: false,
              showRightHandle: false
            }
          };
        })
      );
      setEdges((eds) =>
        eds.map((edge) => {
          let edgeOpacity = 0.15;
          if (focusTypeVisible) {
            const srcVisible = focusTypeVisible.has(edge.source);
            const tgtVisible = focusTypeVisible.has(edge.target);
            edgeOpacity = srcVisible && tgtVisible ? 0.15 : 0;
          }
          return {
            ...edge,
            animated: false,
            style: {
              stroke: "var(--muted-foreground)",
              strokeWidth: 1,
              opacity: edgeOpacity,
              strokeDasharray: "none"
            }
          };
        })
      );
      return;
    }

    const maxDepth = expandAll ? undefined : 1;
    const connected = getConnectedNodes(neighbors, [focusedNodeId], maxDepth);

    const leftHandle = new Set<string>();
    const rightHandle = new Set<string>();
    for (const edge of data.edges) {
      if (connected.has(edge.source) && connected.has(edge.target)) {
        rightHandle.add(edge.source);
        leftHandle.add(edge.target);
      }
    }

    setNodes((nds) =>
      nds.map((node) => {
        const isVisible = connected.has(node.id);
        return {
          ...node,
          data: {
            ...node.data,
            opacity: isVisible ? 1 : 0,
            showLeftHandle: leftHandle.has(node.id),
            showRightHandle: rightHandle.has(node.id)
          }
        };
      })
    );

    setEdges((eds) =>
      eds.map((edge) => {
        const bothVisible = connected.has(edge.source) && connected.has(edge.target);
        const isDirect = edge.source === focusedNodeId || edge.target === focusedNodeId;
        return {
          ...edge,
          animated: bothVisible,
          style: {
            stroke: "var(--muted-foreground)",
            strokeWidth: isDirect ? 1.5 : 1,
            opacity: bothVisible ? (isDirect ? 0.5 : 0.25) : 0,
            strokeDasharray: bothVisible ? "6 4" : "none"
          }
        };
      })
    );
  }, [
    focusedNodeId,
    focusType,
    focusTypeVisible,
    expandAll,
    data.edges,
    neighbors,
    setNodes,
    setEdges
  ]);

  return {
    focusedNodeId,
    selectedNode,
    focusType,
    expandAll,
    changeFocusType,
    setExpandAll,
    handleNodeClick,
    handlePaneClick,
    resetView
  };
}
