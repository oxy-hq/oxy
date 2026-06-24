import { Handle, type NodeProps, Position } from "@xyflow/react";
import { X } from "lucide-react";
import { cn } from "@/libs/shadcn/utils";
import type { WmComputedMeasure, WorldModelEntity } from "@/types/worldModel";
import { EXPANDED_NODE_WIDTH, measureSymbol, measureSymbolColor } from "../worldModelLayout";
import { WorldModelDriverTreeLive } from "./WorldModelDriverTree";

interface WmExpandedEntityData {
  entity: WorldModelEntity;
  seedComputedMeasures?: WmComputedMeasure[] | null;
  /** When set, the node renders the driver-tree breakdown for this measure. */
  breakdownMeasure?: string | null;
  /** Instance key the breakdown is valued at (the active filter seed). */
  instanceKey?: string | null;
  onExpandEntity?: (id: string | null) => void;
}

function formatMeasureValue(raw: string): string {
  const n = Number(raw);
  if (!Number.isFinite(n)) return raw;
  if (Number.isInteger(n)) return n.toLocaleString();
  const formatted = n.toPrecision(7).replace(/\.?0+$/, "");
  return Number(formatted).toLocaleString(undefined, { maximumFractionDigits: 4 });
}

const NON_ADDITIVE_LABELS: Record<string, string> = {
  average: "avg: carries (sum, count)",
  median: "median: carries full multiset",
  count_distinct: "count_distinct: carries set",
  count_distinct_approx: "count_distinct_approx: carries sketch"
};

export function WorldModelExpandedEntityNode({ data }: NodeProps) {
  const { entity, seedComputedMeasures, breakdownMeasure, instanceKey, onExpandEntity } =
    data as unknown as WmExpandedEntityData;

  return (
    <>
      <Handle id='top-in' type='target' position={Position.Top} className='opacity-0' />
      <Handle id='top-out' type='source' position={Position.Top} className='opacity-0' />
      <div
        className='flex flex-col border-2 border-info/80 bg-card shadow-[0_0_32px_rgba(96,165,250,0.22)]'
        style={{ width: EXPANDED_NODE_WIDTH }}
        data-testid={`wm-entity-expanded-${entity.id}`}
      >
        {/* Header */}
        <div className='flex items-center justify-between gap-2 border-info/30 border-b bg-info/5 px-3 py-2'>
          <div className='min-w-0 flex-1'>
            <div className='truncate font-medium text-[13px] text-foreground'>{entity.label}</div>
            <div className='font-mono text-[9px] text-info uppercase tracking-wider'>
              {entity.id} · depth {entity.depth}
            </div>
          </div>
          <button
            type='button'
            onClick={() => onExpandEntity?.(null)}
            className='shrink-0 p-1 text-muted-foreground transition-colors hover:text-foreground'
            title='Collapse'
          >
            <X className='size-3.5' />
          </button>
        </div>

        {breakdownMeasure ? (
          <div className='flex flex-col gap-2 px-3 py-2'>
            <WorldModelDriverTreeLive
              entityId={entity.id}
              keyValue={instanceKey ?? null}
              measure={breakdownMeasure}
            />
          </div>
        ) : (
          <FlatMeasureList entity={entity} measures={seedComputedMeasures ?? []} />
        )}
      </div>
      <Handle id='bottom-in' type='target' position={Position.Bottom} className='opacity-0' />
      <Handle id='bottom-out' type='source' position={Position.Bottom} className='opacity-0' />
    </>
  );
}

// ── Flat list (default, no active breakdown) ───────────────────────────────────

function FlatMeasureList({
  entity,
  measures
}: {
  entity: WorldModelEntity;
  measures: WmComputedMeasure[];
}) {
  return (
    <div className='flex flex-col divide-y divide-border/60'>
      {measures.length === 0 ? (
        <div className='px-3 py-3 font-mono text-[10px] text-muted-foreground'>
          no computed measures for this instance
        </div>
      ) : (
        measures.map((m) => {
          const def =
            entity.own_measures.find((d) => d.name === m.name) ??
            entity.induced_measures.find((d) => d.name === m.name);
          const isNonAdditive = def?.additivity === "non_additive";
          const nonAddLabel = NON_ADDITIVE_LABELS[m.measure_type];

          return (
            <div key={m.name} className='flex flex-col gap-0.5 px-3 py-2'>
              <div className='flex min-w-0 items-center gap-2'>
                <span
                  className={cn(
                    "w-4 shrink-0 text-center font-mono text-[12px] leading-none",
                    measureSymbolColor(m.measure_type)
                  )}
                >
                  {measureSymbol(m.measure_type)}
                </span>
                <span className='min-w-0 flex-1 truncate font-mono text-[11px] text-foreground'>
                  {m.label ?? m.name}
                </span>
                {m.value === null ? (
                  <span className='h-3 w-16 animate-pulse rounded bg-muted' />
                ) : (
                  <span className='shrink-0 font-mono text-[13px] text-info tabular-nums'>
                    {formatMeasureValue(m.value)}
                  </span>
                )}
              </div>
              <div className='flex items-center gap-2 pl-6 font-mono text-[9px] text-muted-foreground'>
                <span>|fiber| = {m.fiber_count}</span>
                {isNonAdditive && (
                  <span className='text-status-warning'>⚠ {nonAddLabel ?? "non-additive"}</span>
                )}
              </div>
            </div>
          );
        })
      )}
    </div>
  );
}
