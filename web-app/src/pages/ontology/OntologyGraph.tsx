import { useMemo, useState, useEffect, useCallback } from "react";
import {
  ReactFlow,
  Node,
  Edge,
  useNodesState,
  useEdgesState,
  Background,
  Panel,
  NodeProps,
  Handle,
  Position,
  ReactFlowProvider,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import {
  OntologyGraph as OntologyGraphType,
  OntologyNode as OntologyNodeType,
} from "@/types/ontology";
import {
  Table2,
  Layout,
  BookOpen,
  Workflow as WorkflowIcon,
  FileCode2,
  Bot,
  Filter,
  Box,
} from "lucide-react";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/shadcn/select";
import { Button } from "@/components/ui/shadcn/button";
import { SidebarTrigger } from "@/components/ui/shadcn/sidebar";
import useSidebar from "@/components/ui/shadcn/sidebar-context";
import { useMediaQuery } from "usehooks-ts";
import { NodeDetailPanel } from "./NodeDetailPanel";

type FocusType =
  | "auto"
  | "agent"
  | "workflow"
  | "topic"
  | "view"
  | "sql_query"
  | "table"
  | "entity";

// Custom node component with explicit handles
const OntologyNode = ({ data }: NodeProps) => {
  const nodeData = data as {
    label: string;
    type: string;
    opacity?: number;
    scale?: number;
  };

  const typeLabels: Record<string, string> = {
    agent: "agent",
    workflow: "automation",
    topic: "topic",
    view: "view",
    sql_query: "sql_query",
    table: "table",
    entity: "entity",
  };

  const icons = {
    agent: <Bot className="w-4 h-4" />,
    workflow: <WorkflowIcon className="w-4 h-4" />,
    topic: <BookOpen className="w-4 h-4" />,
    view: <Layout className="w-4 h-4" />,
    sql_query: <FileCode2 className="w-4 h-4" />,
    table: <Table2 className="w-4 h-4" />,
    entity: <Box className="w-4 h-4" />,
  };

  const bgColors = {
    agent: "#fee2e2",
    workflow: "#ffedd5",
    topic: "#f3e8ff",
    view: "#dcfce7",
    sql_query: "#fef3c7",
    table: "#dbeafe",
    entity: "#fce7f3",
  };

  const borderColors = {
    agent: "#fca5a5",
    workflow: "#fdba74",
    topic: "#d8b4fe",
    view: "#86efac",
    sql_query: "#fcd34d",
    table: "#93c5fd",
    entity: "#f9a8d4",
  };

  const opacity = nodeData.opacity ?? 1;
  const scale = nodeData.scale ?? 1;

  return (
    <div
      style={{
        position: "relative",
        width: "fit-content",
        opacity,
        transform: `scale(${scale})`,
        transition: "opacity 0.3s ease, transform 0.3s ease",
        pointerEvents: opacity === 0 ? "none" : "auto",
        zIndex: 10,
      }}
    >
      {/* Invisible handles centered for connections */}
      <Handle
        type="target"
        position={Position.Left}
        style={{
          opacity: 0,
          width: 1,
          height: 1,
          border: "none",
          background: "transparent",
          left: "50%",
          top: "50%",
          transform: "translate(-50%, -50%)",
        }}
      />
      <Handle
        type="source"
        position={Position.Right}
        style={{
          opacity: 0,
          width: 1,
          height: 1,
          border: "none",
          background: "transparent",
          left: "50%",
          top: "50%",
          transform: "translate(-50%, -50%)",
        }}
      />
      <div
        style={{
          padding: "8px 12px",
          borderRadius: "8px",
          border: `2px solid ${borderColors[nodeData.type as keyof typeof borderColors]}`,
          background: bgColors[nodeData.type as keyof typeof bgColors],
          display: "flex",
          alignItems: "center",
          gap: "8px",
          color: "#000",
          cursor: "pointer",
          position: "relative",
          zIndex: 1,
        }}
      >
        {icons[nodeData.type as keyof typeof icons]}
        <div style={{ fontWeight: 600, fontSize: "14px", color: "#000" }}>
          {nodeData.label}
        </div>
        <span
          style={{
            fontSize: "11px",
            padding: "2px 6px",
            borderRadius: "4px",
            border: `1px solid ${borderColors[nodeData.type as keyof typeof borderColors]}`,
            background: "white",
            color: "#000",
          }}
        >
          {typeLabels[nodeData.type] || nodeData.type}
        </span>
      </div>
    </div>
  );
};

const nodeTypes = {
  ontology: OntologyNode,
};

interface OntologyGraphProps {
  data: OntologyGraphType;
}

function OntologyGraphInner({ data }: OntologyGraphProps) {
  // Focus state management
  const [focusType, setFocusType] = useState<FocusType>(() => {
    const saved = localStorage.getItem("ontology-focus-type");
    return (saved as FocusType) || "auto";
  });

  const [focusedNodeId, setFocusedNodeId] = useState<string | null>(null);
  const [selectedNode, setSelectedNode] = useState<OntologyNodeType | null>(
    null,
  );
  const [expandAll, setExpandAll] = useState<boolean>(() => {
    const saved = localStorage.getItem("ontology-expand-all");
    return saved === "true";
  });

  // Sidebar state
  const { open } = useSidebar();
  const isMobile = useMediaQuery("(max-width: 767px)");

  // Save expand all preference to localStorage
  useEffect(() => {
    localStorage.setItem("ontology-expand-all", expandAll.toString());
  }, [expandAll]);

  // Save focus type to localStorage
  useEffect(() => {
    localStorage.setItem("ontology-focus-type", focusType);
  }, [focusType]);

  // Build connection graph for filtering
  const connectionGraph = useMemo(() => {
    const graph = new Map<string, Set<string>>();
    data.edges.forEach((edge) => {
      if (!graph.has(edge.source)) {
        graph.set(edge.source, new Set());
      }
      if (!graph.has(edge.target)) {
        graph.set(edge.target, new Set());
      }
      graph.get(edge.source)!.add(edge.target);
      graph.get(edge.target)!.add(edge.source);
    });
    return graph;
  }, [data.edges]);

  // Get all connected nodes using BFS with optional depth limit
  const getConnectedNodes = useCallback(
    (nodeIds: string[], maxDepth?: number): Set<string> => {
      const connected = new Set<string>();
      const queue: Array<{ id: string; depth: number }> = nodeIds.map((id) => ({
        id,
        depth: 0,
      }));

      while (queue.length > 0) {
        const current = queue.shift()!;
        if (connected.has(current.id)) continue;
        connected.add(current.id);

        // If maxDepth is specified and we've reached it, don't explore neighbors
        if (maxDepth !== undefined && current.depth >= maxDepth) {
          continue;
        }

        const neighbors = connectionGraph.get(current.id);
        if (neighbors) {
          neighbors.forEach((neighborId) => {
            if (!connected.has(neighborId)) {
              queue.push({ id: neighborId, depth: current.depth + 1 });
            }
          });
        }
      }

      return connected;
    },
    [connectionGraph],
  );

  // Handle node click
  const handleNodeClick = useCallback(
    (_event: React.MouseEvent, node: { id: string }) => {
      setFocusedNodeId((prev) => (prev === node.id ? null : node.id));
      // Find the ontology node data to pass to the detail panel
      const ontologyNode = data.nodes.find((n) => n.id === node.id);
      setSelectedNode(ontologyNode || null);
    },
    [data.nodes],
  );

  // Determine which nodes to show based on focus
  const visibleNodes = useMemo(() => {
    if (focusType === "auto") {
      return new Set(data.nodes.map((n) => n.id));
    }

    // Get nodes of the focused type
    const focusedTypeNodes = data.nodes
      .filter((n) => n.type === focusType)
      .map((n) => n.id);

    // Get all nodes connected to focused type
    return getConnectedNodes(focusedTypeNodes);
  }, [focusType, data.nodes, getConnectedNodes]);

  // Helper function to calculate node styling based on focus
  const getNodeStyle = useCallback(
    (node: OntologyNodeType) => {
      let opacity = 1;
      let scale = 1;

      if (focusType !== "auto") {
        const isFocusedType = node.type === focusType;
        const isConnected = visibleNodes.has(node.id);

        if (!isFocusedType) {
          if (isConnected) {
            opacity = 0.5;
            scale = 0.6;
          } else {
            opacity = 0;
            scale = 0.5;
          }
        }
      }

      return { opacity, scale };
    },
    [focusType, visibleNodes],
  );

  // Helper function to group nodes into rows based on width
  const groupNodesIntoRows = useCallback(
    (
      nodeInfos: Array<{ node: OntologyNodeType; estimatedWidth: number }>,
      maxRowWidth: number,
      padding: number,
    ) => {
      const rows: Array<typeof nodeInfos> = [];
      let currentRow: typeof nodeInfos = [];
      let currentRowWidth = 0;

      nodeInfos.forEach((nodeInfo) => {
        const nodeWidthWithPadding = nodeInfo.estimatedWidth + padding;

        if (
          currentRowWidth + nodeWidthWithPadding > maxRowWidth &&
          currentRow.length > 0
        ) {
          rows.push(currentRow);
          currentRow = [nodeInfo];
          currentRowWidth = nodeWidthWithPadding;
        } else {
          currentRow.push(nodeInfo);
          currentRowWidth += nodeWidthWithPadding;
        }
      });

      if (currentRow.length > 0) {
        rows.push(currentRow);
      }

      return rows;
    },
    [],
  );

  // Helper function to convert a single row into ReactFlow nodes
  const createNodesFromRow = useCallback(
    (
      row: Array<{ node: OntologyNodeType; estimatedWidth: number }>,
      rowIndex: number,
      padding: number,
      rowHeight: number,
    ): Node[] => {
      const totalRowWidth =
        row.reduce((sum, info) => sum + info.estimatedWidth + padding, 0) -
        padding;

      let currentX = -totalRowWidth / 2;
      const nodes: Node[] = [];

      row.forEach(({ node, estimatedWidth }) => {
        const { opacity, scale } = getNodeStyle(node);

        nodes.push({
          id: node.id,
          type: "ontology",
          data: {
            label: node.label,
            type: node.type,
            opacity,
            scale,
          },
          position: {
            x: currentX,
            y: rowIndex * rowHeight,
          },
          zIndex: 10,
        });

        currentX += estimatedWidth + padding;
      });

      return nodes;
    },
    [getNodeStyle],
  );

  // Transform ontology data into ReactFlow nodes
  const initialNodes = useMemo(() => {
    const typeGroups: Record<string, typeof data.nodes> = {};
    data.nodes.forEach((node) => {
      if (!typeGroups[node.type]) {
        typeGroups[node.type] = [];
      }
      typeGroups[node.type].push(node);
    });

    // Determine type order based on focus
    let types = [
      "agent",
      "workflow",
      "topic",
      "view",
      "sql_query",
      "table",
      "entity",
    ];

    if (focusType !== "auto") {
      const otherTypes = types.filter((t) => t !== focusType);
      const halfLength = Math.floor(otherTypes.length / 2);
      types = [
        ...otherTypes.slice(0, halfLength),
        focusType,
        ...otherTypes.slice(halfLength),
      ];
    }

    const rowHeight = 150;
    const minNodeWidth = 150;
    const padding = 50;
    const maxRowWidth = 1400;

    const reactFlowNodes: Node[] = [];
    let globalRowIndex = 0;

    types.forEach((type) => {
      const nodesOfType = typeGroups[type] || [];

      const nodeInfos = nodesOfType.map((node) => {
        const label = node.label || "";
        const estimatedWidth = Math.max(minNodeWidth, label.length * 8 + 100);
        return { node, estimatedWidth };
      });

      const rows = groupNodesIntoRows(nodeInfos, maxRowWidth, padding);

      rows.forEach((row) => {
        const rowNodes = createNodesFromRow(
          row,
          globalRowIndex,
          padding,
          rowHeight,
        );
        reactFlowNodes.push(...rowNodes);
        globalRowIndex++;
      });
    });

    return reactFlowNodes;
  }, [
    data.nodes,
    focusType,
    visibleNodes,
    getNodeStyle,
    groupNodesIntoRows,
    createNodesFromRow,
  ]);

  // Transform ontology edges into ReactFlow edges
  const initialEdges = useMemo(() => {
    // Create a map of node visibility for quick lookup
    const nodeVisibilityMap = new Map<string, boolean>();

    initialNodes.forEach((node) => {
      const opacity = (node.data as { opacity?: number }).opacity ?? 1;
      nodeVisibilityMap.set(node.id, opacity > 0);
    });

    return data.edges.map((edge) => {
      const sourceVisible = nodeVisibilityMap.get(edge.source) ?? true;
      const targetVisible = nodeVisibilityMap.get(edge.target) ?? true;
      const edgeVisible = sourceVisible && targetVisible;

      return {
        id: edge.id,
        source: edge.source,
        target: edge.target,
        type: "straight",
        style: {
          stroke: "#9ca3af",
          strokeWidth: 2,
          opacity: edgeVisible ? 1 : 0,
          transition: "opacity 0.3s ease",
        },
        zIndex: 0,
      };
    });
  }, [data.edges, initialNodes]);

  const [nodes, setNodes, onNodesChange] = useNodesState(initialNodes);
  const [edges, setEdges, onEdgesChange] = useEdgesState<Edge>(initialEdges);

  // Update nodes when focus type or data changes
  useEffect(() => {
    setNodes(initialNodes);
  }, [initialNodes, setNodes]);

  // Update node styles when focused node changes (without recreating nodes)
  useEffect(() => {
    if (!focusedNodeId) {
      // Reset all nodes to full opacity/scale
      setNodes((nds) =>
        nds.map((node) => ({
          ...node,
          data: {
            ...node.data,
            opacity: 1,
            scale: 1,
          },
        })),
      );
      return;
    }

    // Update opacity/scale based on focused node
    // Use depth=1 for direct connections only, or unlimited for expand all
    const maxDepth = expandAll ? undefined : 1;
    const connectedToFocused = getConnectedNodes([focusedNodeId], maxDepth);
    setNodes((nds) =>
      nds.map((node) => {
        if (node.id === focusedNodeId) {
          return {
            ...node,
            data: {
              ...node.data,
              opacity: 1,
              scale: 1,
            },
          };
        } else if (connectedToFocused.has(node.id)) {
          return {
            ...node,
            data: {
              ...node.data,
              opacity: 0.7,
              scale: 0.8,
            },
          };
        } else {
          return {
            ...node,
            data: {
              ...node.data,
              opacity: 0,
              scale: 0.6,
            },
          };
        }
      }),
    );
  }, [focusedNodeId, expandAll, getConnectedNodes, setNodes]);

  // Update edges when visibility changes
  useEffect(() => {
    setEdges(initialEdges);
  }, [initialEdges, setEdges]);

  // Update edge visibility when focused node changes
  useEffect(() => {
    // Build node opacity map for efficient lookup
    const nodeOpacityMap = new Map<string, number>();
    nodes.forEach((node) => {
      const opacity = (node.data as { opacity?: number })?.opacity ?? 1;
      nodeOpacityMap.set(node.id, opacity);
    });

    setEdges((eds) =>
      eds.map((edge) => {
        const sourceOpacity = nodeOpacityMap.get(edge.source) ?? 1;
        const targetOpacity = nodeOpacityMap.get(edge.target) ?? 1;
        const edgeVisible = sourceOpacity > 0 && targetOpacity > 0;

        return {
          ...edge,
          style: {
            ...edge.style,
            opacity: edgeVisible ? 1 : 0,
          },
        };
      }),
    );
  }, [nodes, setEdges]);

  const nodeCount = data.nodes.length;
  const edgeCount = data.edges.length;

  const typeCounts = useMemo(() => {
    const counts: Record<string, number> = {};
    data.nodes.forEach((node) => {
      counts[node.type] = (counts[node.type] || 0) + 1;
    });
    return counts;
  }, [data.nodes]);

  const typeLabels: Record<string, string> = {
    agent: "Agents",
    workflow: "Automations",
    topic: "Topics",
    view: "Views",
    sql_query: "SQL Queries",
    table: "Tables",
    entity: "Entities",
  };

  return (
    <div style={{ width: "100vw", height: "100vh", position: "relative" }}>
      {(!open || isMobile) && (
        <div
          style={{
            position: "absolute",
            top: "16px",
            left: "16px",
            zIndex: 1000,
          }}
        >
          <SidebarTrigger />
        </div>
      )}
      <ReactFlow
        nodes={nodes}
        edges={edges}
        onNodesChange={onNodesChange}
        onEdgesChange={onEdgesChange}
        onNodeClick={handleNodeClick}
        nodeTypes={nodeTypes}
        elementsSelectable={false}
        nodesConnectable={false}
        nodesDraggable={false}
        fitView
        fitViewOptions={{ padding: 0.2 }}
        minZoom={0.1}
        maxZoom={2}
        proOptions={{ hideAttribution: true }}
        style={{ width: "100%", height: "100%" }}
      >
        <Background />
        <Panel
          position="top-left"
          className="bg-sidebar-background border border-sidebar-border rounded-lg shadow-lg p-4"
          style={
            !open || isMobile
              ? { marginTop: "56px", transition: "margin-top 0.2s ease" }
              : { transition: "margin-top 0.2s ease" }
          }
        >
          <div className="text-sm font-semibold mb-2 text-sidebar-foreground">
            Ontology Overview
          </div>
          <div
            className="space-y-1 text-sm text-sidebar-foreground/70"
            data-testid="ontology-stats"
          >
            <div className="flex justify-between gap-4">
              <span>Total Nodes:</span>
              <span
                className="font-medium text-sidebar-foreground"
                data-testid="ontology-total-nodes"
              >
                {nodeCount}
              </span>
            </div>
            <div className="flex justify-between gap-4">
              <span>Total Edges:</span>
              <span
                className="font-medium text-sidebar-foreground"
                data-testid="ontology-total-edges"
              >
                {edgeCount}
              </span>
            </div>
            <div className="border-t border-sidebar-border pt-2 mt-2">
              {Object.entries(typeCounts).map(([type, count]) => (
                <div key={type} className="flex justify-between gap-4">
                  <span>{typeLabels[type] || type}:</span>
                  <span className="font-medium text-sidebar-foreground">
                    {count}
                  </span>
                </div>
              ))}
            </div>
          </div>

          <div className="border-t border-sidebar-border pt-3 mt-3">
            <div className="flex items-center gap-2 mb-2">
              <Filter className="w-4 h-4 text-sidebar-foreground/70" />
              <span className="text-sm font-semibold text-sidebar-foreground">
                Focus View
              </span>
            </div>
            <Select
              value={focusType}
              onValueChange={(value) => {
                setFocusType(value as FocusType);
                setFocusedNodeId(null);
              }}
            >
              <SelectTrigger
                className="h-9 text-sm bg-sidebar-accent border-sidebar-border text-sidebar-foreground"
                data-testid="ontology-filter-type"
              >
                <SelectValue placeholder="Select focus" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="auto" className="text-sm">
                  <div className="flex items-center gap-2">
                    <span>All Types</span>
                  </div>
                </SelectItem>
                <SelectItem value="agent" className="text-sm">
                  <div className="flex items-center gap-2">
                    <Bot className="w-4 h-4" />
                    <span>Agents</span>
                  </div>
                </SelectItem>
                <SelectItem value="workflow" className="text-sm">
                  <div className="flex items-center gap-2">
                    <WorkflowIcon className="w-4 h-4" />
                    <span>Automations</span>
                  </div>
                </SelectItem>
                <SelectItem value="topic" className="text-sm">
                  <div className="flex items-center gap-2">
                    <BookOpen className="w-4 h-4" />
                    <span>Topics</span>
                  </div>
                </SelectItem>
                <SelectItem value="view" className="text-sm">
                  <div className="flex items-center gap-2">
                    <Layout className="w-4 h-4" />
                    <span>Views</span>
                  </div>
                </SelectItem>
                <SelectItem value="sql_query" className="text-sm">
                  <div className="flex items-center gap-2">
                    <FileCode2 className="w-4 h-4" />
                    <span>SQL Queries</span>
                  </div>
                </SelectItem>
                <SelectItem value="table" className="text-sm">
                  <div className="flex items-center gap-2">
                    <Table2 className="w-4 h-4" />
                    <span>Tables</span>
                  </div>
                </SelectItem>
                <SelectItem value="entity" className="text-sm">
                  <div className="flex items-center gap-2">
                    <Box className="w-4 h-4" />
                    <span>Entities</span>
                  </div>
                </SelectItem>
              </SelectContent>
            </Select>

            {/* Expansion mode toggle */}
            <div className="mt-2 pt-2 border-t border-sidebar-border">
              <label
                className={`flex items-center gap-2 ${
                  focusedNodeId
                    ? "cursor-pointer"
                    : "cursor-not-allowed opacity-50"
                }`}
              >
                <input
                  type="checkbox"
                  checked={expandAll}
                  onChange={(e) => setExpandAll(e.target.checked)}
                  disabled={!focusedNodeId}
                  className="w-4 h-4 rounded border-gray-300 text-primary focus:ring-primary disabled:cursor-not-allowed"
                />
                <span className="text-sm">Expand all connected</span>
              </label>
              <p className="text-xs text-muted-foreground mt-1">
                Show entire cluster when clicked
              </p>
            </div>

            {focusedNodeId && (
              <div className="mt-2 pt-2 border-t border-sidebar-border">
                <Button
                  onClick={() => {
                    setFocusedNodeId(null);
                  }}
                  variant="outline"
                  size="sm"
                  className="w-full text-sm"
                >
                  Reset View
                </Button>
              </div>
            )}
          </div>
        </Panel>
      </ReactFlow>
      <NodeDetailPanel
        node={selectedNode}
        onClose={() => setSelectedNode(null)}
      />
    </div>
  );
}

export function OntologyGraph(props: OntologyGraphProps) {
  return (
    <ReactFlowProvider>
      <OntologyGraphInner {...props} />
    </ReactFlowProvider>
  );
}
