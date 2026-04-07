import {
  Background,
  BackgroundVariant,
  type Edge,
  ReactFlow,
  ReactFlowProvider,
  useEdgesState,
  useNodesState
} from "@xyflow/react";
import { useMemo } from "react";
import "@xyflow/react/dist/style.css";
import type { ContextGraph as ContextGraphType } from "@/types/contextGraph";
import { ContextGraphNode } from "./components/ContextGraphNode";
import { GraphControlPanel } from "./components/GraphControlPanel";
import { NodeDetailPanel } from "./components/NodeDetailPanel";
import { buildInitialEdges, buildInitialNodes } from "./layout";
import { useGraphFocus } from "./useGraphFocus";

const nodeTypes = { "context-graph": ContextGraphNode };

interface ContextGraphProps {
  data: ContextGraphType;
}

function ContextGraphInner({ data }: ContextGraphProps) {
  const initialNodes = useMemo(() => buildInitialNodes(data.nodes), [data.nodes]);
  const initialEdges = useMemo(() => buildInitialEdges(data.edges), [data.edges]);

  const [nodes, setNodes, onNodesChange] = useNodesState(initialNodes);
  const [edges, setEdges, onEdgesChange] = useEdgesState<Edge>(initialEdges);

  const {
    focusedNodeId,
    selectedNode,
    focusType,
    expandAll,
    changeFocusType,
    setExpandAll,
    handleNodeClick,
    handlePaneClick,
    resetView
  } = useGraphFocus({ data, setNodes, setEdges });

  const typeCounts = useMemo(() => {
    const counts: Record<string, number> = {};
    for (const node of data.nodes) {
      counts[node.type] = (counts[node.type] || 0) + 1;
    }
    return counts;
  }, [data.nodes]);

  return (
    <div style={{ width: "100%", height: "100vh", position: "relative" }}>
      <ReactFlow
        nodes={nodes}
        edges={edges}
        onNodesChange={onNodesChange}
        onEdgesChange={onEdgesChange}
        onNodeClick={handleNodeClick}
        onPaneClick={handlePaneClick}
        nodeTypes={nodeTypes}
        elementsSelectable={false}
        nodesConnectable={false}
        nodesDraggable={false}
        fitView
        fitViewOptions={{ padding: 0.2 }}
        minZoom={0.1}
        maxZoom={2}
        proOptions={{ hideAttribution: true }}
        style={{ width: "100%", height: "100%", background: "var(--background)" }}
      >
        <Background color='var(--muted-foreground)' variant={BackgroundVariant.Dots} />
        <GraphControlPanel
          nodes={data.nodes}
          edges={data.edges}
          typeCounts={typeCounts}
          focusType={focusType}
          onFocusTypeChange={changeFocusType}
          expandAll={expandAll}
          onExpandAllChange={setExpandAll}
          focusedNodeId={focusedNodeId}
          onReset={resetView}
        />
      </ReactFlow>
      <NodeDetailPanel node={selectedNode} onClose={resetView} />
    </div>
  );
}

export function ContextGraph(props: ContextGraphProps) {
  return (
    <ReactFlowProvider>
      <ContextGraphInner {...props} />
    </ReactFlowProvider>
  );
}
