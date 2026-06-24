import { Handle, type NodeProps, Position } from "@xyflow/react";
import { ChevronRight } from "lucide-react";
import { cn } from "@/libs/shadcn/utils";
import type { WmComputedMeasure, WorldModelEntity } from "@/types/worldModel";
import { measureSymbol, measureSymbolColor, NODE_WIDTH } from "../worldModelLayout";

interface WmEntityData {
  entity: WorldModelEntity;
  selected: boolean;
  dimmed: boolean;
  filterCount?: { matched: number; total: number } | null;
  isCountLoading?: boolean;
  /** Provided only for the filter-seed entity when a filter is active. */
  seedComputedMeasures?: WmComputedMeasure[] | null;
  /** Expand this entity into the driver-tree breakdown for `measure`. */
  onExpandEntity?: (id: string | null, measure?: string | null) => void;
}

function formatMeasureValue(raw: string): string {
  const n = Number(raw);
  if (!Number.isFinite(n)) return raw;
  if (Number.isInteger(n)) return n.toLocaleString();
  const formatted = n.toPrecision(7).replace(/\.?0+$/, "");
  return Number(formatted).toLocaleString(undefined, { maximumFractionDigits: 4 });
}

function MetricChipsSection({
  measures,
  entity,
  onExpandEntity
}: {
  measures: WmComputedMeasure[];
  entity: WorldModelEntity;
  onExpandEntity?: (id: string | null, measure?: string | null) => void;
}) {
  if (measures.length === 0) {
    return (
      <div className='mt-0.5 border-border border-t pt-1 font-mono text-[9px] text-muted-foreground'>
        no measures
      </div>
    );
  }

  return (
    <div className='mt-0.5 flex flex-col gap-0.5 border-border border-t pt-1'>
      {measures.map((m) => {
        const def =
          entity.own_measures.find((d) => d.name === m.name) ??
          entity.induced_measures.find((d) => d.name === m.name);
        const isNonAdditive = def?.additivity === "non_additive";
        const hasBreakdown = def?.has_breakdown === true;

        return (
          <div
            key={m.name}
            className='group flex min-w-0 items-center gap-1 font-mono text-[9.5px]'
          >
            <span
              className={cn(
                "shrink-0 text-[10px] leading-none",
                measureSymbolColor(m.measure_type)
              )}
            >
              {measureSymbol(m.measure_type)}
            </span>
            <span className='min-w-0 flex-1 truncate text-muted-foreground'>
              {m.label ?? m.name}
            </span>
            {m.value === null ? (
              <span className='h-2 w-10 animate-pulse rounded bg-muted' />
            ) : (
              <span className='shrink-0 text-foreground tabular-nums'>
                {formatMeasureValue(m.value)}
              </span>
            )}
            {isNonAdditive && (
              <span className='shrink-0 text-[8px] text-status-warning' title='non-additive'>
                ⚠
              </span>
            )}
            {onExpandEntity && hasBreakdown && (
              <button
                type='button'
                className='shrink-0 text-info transition-colors hover:text-foreground'
                title='Break down at this instance'
                data-testid={`wm-measure-zoom-${entity.id}-${m.name}`}
                onClick={(e) => {
                  e.stopPropagation();
                  onExpandEntity(entity.id, m.name);
                }}
              >
                <ChevronRight className='size-3' />
              </button>
            )}
          </div>
        );
      })}
    </div>
  );
}

export function WorldModelEntityNode({ data }: NodeProps) {
  const {
    entity,
    selected,
    dimmed,
    filterCount,
    isCountLoading,
    seedComputedMeasures,
    onExpandEntity
  } = data as unknown as WmEntityData;

  const obsCount = entity.dimensions.length;
  const calcCount = entity.own_measures.length + entity.induced_measures.length;
  const isUnreachable = !!filterCount && filterCount.matched === 0;
  const hasChips = !!seedComputedMeasures;

  return (
    <>
      <Handle id='top-in' type='target' position={Position.Top} className='opacity-0' />
      <Handle id='top-out' type='source' position={Position.Top} className='opacity-0' />
      <div
        className={cn(
          "flex cursor-pointer select-none flex-col gap-1 border border-info/60 bg-card p-2",
          "transition-all duration-250 ease-out",
          "hover:shadow-[0_0_20px_rgba(96,165,250,0.18)]",
          selected && "shadow-[0_0_26px_rgba(96,165,250,0.32)] ring-2 ring-info/60",
          dimmed && "opacity-35"
        )}
        style={{ width: NODE_WIDTH }}
        data-testid={`wm-entity-${entity.id}`}
      >
        {/* Row 1: entity name */}
        <div className='flex items-baseline justify-between gap-1.5'>
          <span className='truncate font-medium text-[12px] text-foreground leading-tight'>
            {entity.label}
          </span>
        </div>

        {/* Row 2: grain id · depth */}
        <div className='flex min-w-0 items-center gap-1.5 font-mono text-[9px] text-muted-foreground'>
          <span className='size-1.5 shrink-0 rounded-full bg-info' />
          <span className='truncate'>{entity.id}</span>
          <span className='shrink-0 opacity-50'>·</span>
          <span className='shrink-0'>depth {entity.depth}</span>
        </div>

        {/* Row 3: metric chips, filter badge, loading skeleton, or obs/calc counts */}
        <div className='mt-0.5 text-[10px]'>
          {hasChips ? (
            <MetricChipsSection
              measures={seedComputedMeasures!}
              entity={entity}
              onExpandEntity={onExpandEntity}
            />
          ) : filterCount ? (
            <div
              className={cn(
                "flex w-full items-baseline justify-between gap-1 border px-1.5 py-0.5 font-mono text-[10px]",
                isUnreachable
                  ? "border-destructive/40 text-muted-foreground"
                  : "border-info/30 text-info"
              )}
            >
              <span className='uppercase tracking-wider'>filter</span>
              <span className='tabular-nums'>
                {filterCount.matched}
                <span className='text-muted-foreground'> / {filterCount.total}</span>
              </span>
            </div>
          ) : isCountLoading ? (
            <div className='flex w-full items-baseline justify-between border border-info/20 px-1.5 py-0.5 font-mono text-[10px] text-info/50'>
              <span className='uppercase tracking-wider'>counting</span>
              <span className='animate-pulse'>···</span>
            </div>
          ) : (
            <div className='flex items-center gap-3'>
              <span className='flex items-center gap-1'>
                <span className='text-success'>●</span>
                <span className='text-foreground'>{obsCount}</span>
                <span className='text-muted-foreground'>obs</span>
              </span>
              <span className='flex items-center gap-1'>
                <span className='font-mono text-[11px] text-[color:var(--vis-purple)]'>Σ</span>
                <span className='text-foreground'>{calcCount}</span>
                <span className='text-muted-foreground'>calc</span>
              </span>
            </div>
          )}
        </div>
      </div>
      <Handle id='bottom-in' type='target' position={Position.Bottom} className='opacity-0' />
      <Handle id='bottom-out' type='source' position={Position.Bottom} className='opacity-0' />
    </>
  );
}
