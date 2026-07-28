import { useMemo, useState } from "react";
import { useMetricTree } from "@/hooks/api/useMetricTree";
import type { WmSelection, WorldModel } from "@/types/worldModel";
import { declaringView } from "./measureTarget";
import { InfoTip, SectionSpinner } from "./panelPrimitives";
import { WorldModelDriversSection } from "./WorldModelDriversSection";
import {
  DEFAULT_PRESET_DAYS,
  WorldModelOpportunitiesSection
} from "./WorldModelOpportunitiesSection";
import { measureNodeSelection } from "./worldModelNav";

interface WorldModelMeasureAnalysisProps {
  measureName: string;
  /** True when the measure is promoted to this grain rather than declared on it. */
  induced: boolean;
  /** Source view of an induced measure — where it is actually declared. */
  promotedFrom?: string;
  /** `view` of the host entity, i.e. where a non-induced measure is declared. */
  entityView: string | undefined;
  /** `additive` | `non_additive` | `passthrough`. */
  additivity: string;
  /** The full graph, for resolving a driver/dimension back to a selectable node. */
  model: WorldModel;
  onSelect: (s: WmSelection) => void;
}

/**
 * The "how do we improve this measure" half of the measure panel.
 *
 * The world model addresses a measure as `(entity, measureName)` while the
 * metric-tree endpoints take a node id (`view.measure`), so this resolves the
 * selection against the real tree once and hands the node id to both sections.
 * A measure with no metric-tree node has neither drivers nor comparable
 * segments, and says so rather than rendering two empty sections.
 *
 * Period preference lives here rather than in the (per-measure keyed) sections,
 * so switching between sibling measures keeps the analyst's window while still
 * resetting each section's transient result state.
 */
export function WorldModelMeasureAnalysis({
  measureName,
  induced,
  promotedFrom,
  entityView,
  additivity,
  model,
  onSelect
}: WorldModelMeasureAnalysisProps) {
  const view = declaringView(induced, promotedFrom, entityView);
  const { data: tree, isPending, error } = useMetricTree();
  const [periodDays, setPeriodDays] = useState(DEFAULT_PRESET_DAYS);

  const node = useMemo(() => {
    if (!tree || !view) return undefined;
    return tree.nodes.find((n) => n.view === view && n.measure === measureName);
  }, [tree, view, measureName]);

  if (isPending) {
    return (
      <section className='border-border border-t pt-3'>
        <SectionSpinner />
      </section>
    );
  }

  // Distinguish a transient tree-load failure from a genuinely-absent node:
  // the former is a network blip, not a modeling gap.
  if (error) {
    return (
      <section className='border-border border-t pt-3'>
        <p className='font-mono text-[10px] text-muted-foreground leading-relaxed'>
          Couldn't load the metric tree, so drivers and comparable segments are unavailable right
          now. Try again in a moment.
        </p>
      </section>
    );
  }

  if (!node) {
    return (
      <section className='border-border border-t pt-3'>
        <p className='font-mono text-[10px] text-muted-foreground leading-relaxed'>
          This measure has no metric-tree node, so it has no drivers to rank and no comparable
          segments.
        </p>
      </section>
    );
  }

  // Resolve a driver's metric-tree node back to a selectable graph node, so a
  // driver row navigates like any other node. `null` → the row stays inert.
  const selectDriver = (driverNodeId: string) => {
    const selection = measureNodeSelection(model, driverNodeId);
    if (selection) onSelect(selection);
  };
  const canSelectDriver = (driverNodeId: string) =>
    measureNodeSelection(model, driverNodeId) != null;

  // Key both sections by node id so switching between sibling measures resets
  // their local state — a stale what-if result or an open segment group from the
  // previous measure must not carry over.
  return (
    <div className='flex flex-col gap-3'>
      <p className='border-border border-t pt-3 font-mono text-[9.5px] text-muted-foreground leading-relaxed'>
        <span className='text-foreground'>Drivers</span> — what structurally moves this measure.{" "}
        <span className='text-foreground'>Opportunities</span> — where today's performance varies,
        what closing the gap would add, and (per row) where that gap comes from.{" "}
        <InfoTip content="Drivers come from the metric tree's modelled relationships (a what-if you reason with); opportunities come from real warehouse data (a gap you observe). They can disagree — the model says what should move it, the data says where it actually varies." />
      </p>
      <WorldModelDriversSection
        key={node.id}
        nodeId={node.id}
        onSelectDriver={selectDriver}
        canSelectDriver={canSelectDriver}
      />
      <WorldModelOpportunitiesSection
        key={node.id}
        nodeId={node.id}
        view={node.view}
        additivity={additivity}
        periodDays={periodDays}
        onPeriodDaysChange={setPeriodDays}
        model={model}
        onSelect={onSelect}
      />
    </div>
  );
}
