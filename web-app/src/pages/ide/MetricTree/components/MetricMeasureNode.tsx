import type { NodeProps } from "@xyflow/react";
import { cn } from "@/libs/shadcn/utils";
import type { MetricNode } from "@/types/metricTree";
import { GraphNodeCard, GraphNodeHandles } from "../../components/semanticGraph";
import { measureDescription, measureTitle } from "../measureTitle";
import { type NodeRole, ROLE_MARKS } from "./nodeRoles";

export interface MetricMeasureData {
  node: MetricNode;
  selected: boolean;
  role: NodeRole;
  /** Outside the selected measure's neighbourhood. */
  dimmed?: boolean;
}

/** A measure on the Metric Tree canvas, drawn in the shared semantic-graph card
 *  so it reads as the same object the World Model shows. */
export function MetricMeasureNode({ data }: NodeProps) {
  const { node, selected, role, dimmed = false } = data as unknown as MetricMeasureData;
  const mark = ROLE_MARKS[role];

  return (
    <>
      <GraphNodeHandles />
      <GraphNodeCard selected={selected} dimmed={dimmed} data-testid={`metric-node-${node.id}`}>
        {/* Row 1: measure name. Never `node.label` — see `measureTitle`. */}
        <div className='flex items-baseline justify-between gap-1.5'>
          <span
            className={cn(
              "truncate font-medium text-[12px] leading-tight",
              dimmed ? "text-muted-foreground" : "text-foreground"
            )}
            title={measureDescription(node) ?? measureTitle(node)}
          >
            {measureTitle(node)}
          </span>
        </div>

        {/* Row 2: owning view · aggregation. The view rather than the qualified
            id, which would just repeat the name already in row 1. */}
        <div className='flex min-w-0 items-center gap-1.5 font-mono text-[9px] text-muted-foreground'>
          <span
            className={cn(
              "size-1.5 shrink-0 rounded-full",
              dimmed ? "bg-muted-foreground" : "bg-info"
            )}
          />
          <span className='truncate' title={node.id}>
            {node.view}
          </span>
          <span className='shrink-0 opacity-50'>·</span>
          <span className='shrink-0'>{node.measure_type}</span>
        </div>

        {/* Row 3: role mark */}
        <div className='mt-0.5 flex items-center gap-1 font-mono text-[10px]'>
          <span className={cn("shrink-0 text-[11px] leading-none", mark.className)}>
            {mark.symbol}
          </span>
          <span className='text-muted-foreground uppercase tracking-wider'>{mark.label}</span>
        </div>
      </GraphNodeCard>
    </>
  );
}
