import { useCallback, useState } from "react";
import { useWmFilterCounts, useWmInstanceDetail, useWorldModel } from "@/hooks/api/useWorldModel";
import type { WmFilterSeed, WmInstance, WmSelection } from "@/types/worldModel";
import { FilterPill } from "./components/FilterPill";
import { InstancePickerPopover } from "./components/InstancePickerPopover";
import { WorldModelDetailPanel } from "./components/WorldModelDetailPanel";
import { WorldModelGraph } from "./components/WorldModelGraph";

export default function WorldModelView() {
  const { data: model, isLoading, error } = useWorldModel();

  // Selection + back-navigation history stack.
  const [selection, setSelectionRaw] = useState<WmSelection>(null);
  const [history, setHistory] = useState<WmSelection[]>([]);

  // Filter seed state.
  const [filterSeed, setFilterSeed] = useState<WmFilterSeed | null>(null);

  // Picker state: which entity + screen position.
  const [pickerState, setPickerState] = useState<{
    entityId: string;
    position: { x: number; y: number };
  } | null>(null);

  // Driver-tree expansion: which entity node is expanded + which measure it breaks down.
  const [expandedEntityId, setExpandedEntityId] = useState<string | null>(null);
  const [breakdownMeasure, setBreakdownMeasure] = useState<string | null>(null);

  const onExpandEntity = useCallback((entityId: string | null, measure?: string | null) => {
    setExpandedEntityId(entityId);
    setBreakdownMeasure(entityId ? (measure ?? null) : null);
  }, []);

  // Stream filter counts lazily when a seed is set — counts appear progressively.
  const { counts: filterCounts, isLoading: isCountLoading } = useWmFilterCounts(
    filterSeed?.entityId ?? null,
    filterSeed?.keyValue ?? null
  );

  // Load computed measures for the filter seed so entity card chips can show values.
  const { data: seedDetail } = useWmInstanceDetail(
    filterSeed?.entityId ?? null,
    filterSeed?.keyValue ?? null
  );

  const onSelect = useCallback((next: WmSelection) => {
    setSelectionRaw((prev) => {
      if (next === null) {
        setHistory([]);
        return null;
      }
      if (prev !== null) {
        setHistory((h) => [...h, prev]);
      }
      return next;
    });
  }, []);

  const onBack = useCallback(() => {
    setHistory((h) => {
      const prev = h[h.length - 1] ?? null;
      setSelectionRaw(prev);
      return h.slice(0, -1);
    });
  }, []);

  const handleOpenPicker = useCallback((entityId: string, position: { x: number; y: number }) => {
    setPickerState({ entityId, position });
  }, []);

  const handlePickInstance = useCallback(
    (inst: WmInstance) => {
      if (!pickerState) return;
      const seed: WmFilterSeed = {
        entityId: pickerState.entityId,
        keyValue: inst.key,
        label: inst.display
      };
      setFilterSeed(seed);
      onSelect({
        kind: "instance",
        entityId: pickerState.entityId,
        keyValue: inst.key,
        label: inst.display
      });
    },
    [pickerState, onSelect]
  );

  const handleClearFilter = useCallback(() => {
    setFilterSeed(null);
    setExpandedEntityId(null);
    setBreakdownMeasure(null);
    setSelectionRaw((prev) => (prev?.kind === "instance" ? null : prev));
    setHistory((h) => h.filter((s) => s?.kind !== "instance"));
  }, []);

  if (isLoading) {
    return (
      <div className='flex h-full items-center justify-center text-muted-foreground text-sm'>
        Loading world model…
      </div>
    );
  }

  if (error || !model) {
    return (
      <div className='flex h-full items-center justify-center text-destructive text-sm'>
        {error instanceof Error ? error.message : "Failed to load world model"}
      </div>
    );
  }

  const pickerEntity = pickerState
    ? model.entities.find((e) => e.id === pickerState.entityId)
    : null;

  return (
    <div className='flex h-full min-h-0 w-full overflow-hidden'>
      <div className='relative min-h-0 flex-1 overflow-hidden'>
        {filterSeed && (
          <FilterPill
            seed={filterSeed}
            isCountLoading={isCountLoading}
            onClear={handleClearFilter}
          />
        )}
        <WorldModelGraph
          model={model}
          selection={selection}
          filterCounts={filterCounts}
          isCountLoading={isCountLoading}
          filterSeedEntityId={filterSeed?.entityId ?? null}
          seedComputedMeasures={seedDetail?.computed_measures ?? null}
          expandedEntityId={expandedEntityId}
          breakdownMeasure={breakdownMeasure}
          instanceKey={filterSeed?.keyValue ?? null}
          onExpandEntity={onExpandEntity}
          onSelectEntity={(id) => onSelect({ kind: "entity", entityId: id })}
          onSelectPromotion={(from, to) => onSelect({ kind: "promotion", from, to })}
          onOpenPicker={handleOpenPicker}
          onClearSelection={() => onSelect(null)}
        />
        {pickerState && pickerEntity && (
          <InstancePickerPopover
            entityId={pickerState.entityId}
            entityLabel={pickerEntity.label}
            position={pickerState.position}
            onPick={handlePickInstance}
            onClose={() => setPickerState(null)}
          />
        )}
      </div>
      <div className='w-96 shrink-0 overflow-hidden border-border border-l'>
        <WorldModelDetailPanel
          model={model}
          selection={selection}
          onSelect={onSelect}
          history={history}
          onBack={onBack}
          seedInstanceDetail={seedDetail ?? null}
        />
      </div>
    </div>
  );
}
