import type { ContextGraphEdge, ContextGraphNode } from "@/types/contextGraph";
import { TYPE_LABELS } from "../../../constants";

interface GraphStatsPanelProps {
  nodes: ContextGraphNode[];
  edges: ContextGraphEdge[];
  typeCounts: Record<string, number>;
  /** See `ContextGraph.tablesUnknown` — the instance could not look, which is
   *  not the same as there being nothing to see. */
  tablesUnknown?: boolean;
}

export function GraphStatsPanel({
  nodes,
  edges,
  typeCounts,
  tablesUnknown = false
}: GraphStatsPanelProps) {
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
          {tablesUnknown && (
            <div className='flex justify-between gap-4' data-testid='context-graph-tables-unknown'>
              <span>Tables:</span>
              <span
                className='font-medium text-sidebar-foreground/60'
                title='This instance has no working copy, so it cannot enumerate database tables. They are not missing — they are not visible from here.'
              >
                not visible from here
              </span>
            </div>
          )}
        </div>
      </div>
    </>
  );
}
