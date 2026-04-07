import { Panel as RFPanel } from "@xyflow/react";
import type { ContextGraphEdge, ContextGraphNode } from "@/types/contextGraph";
import type { FocusType } from "../../constants";
import { GraphFilterPanel } from "./components/GraphFilterPanel";
import { GraphStatsPanel } from "./components/GraphStatsPanel";

interface GraphControlPanelProps {
  nodes: ContextGraphNode[];
  edges: ContextGraphEdge[];
  typeCounts: Record<string, number>;
  focusType: FocusType;
  onFocusTypeChange: (type: FocusType) => void;
  expandAll: boolean;
  onExpandAllChange: (expand: boolean) => void;
  focusedNodeId: string | null;
  onReset: () => void;
}

export function GraphControlPanel({
  nodes,
  edges,
  typeCounts,
  focusType,
  onFocusTypeChange,
  expandAll,
  onExpandAllChange,
  focusedNodeId,
  onReset
}: GraphControlPanelProps) {
  return (
    <RFPanel
      position='top-left'
      className='rounded-lg border border-sidebar-border bg-sidebar-background p-4 shadow-lg'
    >
      <GraphStatsPanel nodes={nodes} edges={edges} typeCounts={typeCounts} />
      <GraphFilterPanel
        focusType={focusType}
        onFocusTypeChange={onFocusTypeChange}
        expandAll={expandAll}
        onExpandAllChange={onExpandAllChange}
        focusedNodeId={focusedNodeId}
        onReset={onReset}
      />
    </RFPanel>
  );
}
