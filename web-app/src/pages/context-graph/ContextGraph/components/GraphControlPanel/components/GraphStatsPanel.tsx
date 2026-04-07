import type { ContextGraphEdge, ContextGraphNode } from "@/types/contextGraph";
import { TYPE_LABELS } from "../../../constants";

interface GraphStatsPanelProps {
  nodes: ContextGraphNode[];
  edges: ContextGraphEdge[];
  typeCounts: Record<string, number>;
}

export function GraphStatsPanel({ nodes, edges, typeCounts }: GraphStatsPanelProps) {
  return (
    <>
      <div className='mb-2 font-semibold text-sidebar-foreground text-sm'>
        Context Graph Overview
      </div>
      <div
        className='space-y-1 text-sidebar-foreground/70 text-sm'
        data-testid='context-graph-stats'
      >
        <div className='flex justify-between gap-4'>
          <span>Total Nodes:</span>
          <span
            className='font-medium text-sidebar-foreground'
            data-testid='context-graph-total-nodes'
          >
            {nodes.length}
          </span>
        </div>
        <div className='flex justify-between gap-4'>
          <span>Total Edges:</span>
          <span
            className='font-medium text-sidebar-foreground'
            data-testid='context-graph-total-edges'
          >
            {edges.length}
          </span>
        </div>
        <div className='mt-2 border-sidebar-border border-t pt-2'>
          {Object.entries(typeCounts).map(([type, count]) => (
            <div key={type} className='flex justify-between gap-4'>
              <span>{TYPE_LABELS[type] || type}:</span>
              <span className='font-medium text-sidebar-foreground'>{count}</span>
            </div>
          ))}
        </div>
      </div>
    </>
  );
}
