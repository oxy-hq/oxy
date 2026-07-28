import { useMemo } from "react";
import { useDrillQuery } from "@/hooks/api/useMetricTree";
import type { DrillRequest, OpportunityInstance } from "@/types/metricTree";
import { SectionSpinner } from "./panelPrimitives";
import { DrillChain } from "./WorldModelDrillChain";
import { presetPeriod } from "./WorldModelOpportunitiesSection";

interface WorldModelSegmentDrillProps {
  /** Metric-tree node id (`view.measure`) — same target the row's own gap was
   *  sized against. */
  nodeId: string;
  /** Time dimension already resolved by the parent section. */
  timeDimension: string;
  /** Trailing-window preset, owned by the parent so it survives measure switches. */
  periodDays: number;
  /**
   * The ranked row's own dimension and segment — becomes the drill's `root`, so
   * the engine decomposes THIS row instead of its own top pick.
   *
   * Omit entirely to send no `root` at all, which is the documented contract for
   * "let the engine pick its own top row": the whole-measure chain the deleted
   * standalone drill section rendered. Used where there are no ranked rows to
   * root against (a value-share mode such as `type: count`).
   */
  root?: { dimension: string; segment: string };
  /** Same instance scope as the Opportunities row this decomposes, when the
   *  panel is instance-scoped. Omit to decompose across the whole population. */
  instance?: OpportunityInstance;
}

/**
 * One ranked row's decomposition. Where the section ranks every first-level cut,
 * this answers "where does THIS row's gap come from" — the same engine call the
 * old standalone Drill section made, but rooted at the row the analyst opened
 * rather than at the engine's own top pick.
 *
 * Mounted only when its row is expanded: the recursive scan is a bounded but
 * non-trivial number of warehouse queries, so an unexpanded row costs nothing.
 * The parent unmounts this on collapse; TanStack Query's cache (5min staleTime,
 * keyed by root) makes re-expanding free rather than a refetch.
 *
 * Gating (`canQuery` / `additivity` / `drillable`) is NOT repeated here — the
 * section above already decided this row is worth a drill affordance before
 * this component is ever mounted.
 */
export function WorldModelSegmentDrill({
  nodeId,
  timeDimension,
  periodDays,
  root,
  instance
}: WorldModelSegmentDrillProps) {
  const request = useMemo<DrillRequest>(
    () => ({
      target: nodeId,
      time_dimension: timeDimension,
      period: presetPeriod(periodDays),
      // Absent `root` = engine picks its own top row. Presence check ensures
      // no partial state can slip through.
      ...(root ? { root } : {}),
      ...(instance ? { instance } : {})
    }),
    [nodeId, timeDimension, periodDays, root, instance]
  );

  const drill = useDrillQuery(request, true);
  const levels = drill.data?.levels;

  if (drill.isPending) return <SectionSpinner label='decomposing the gap…' />;

  if (drill.error) {
    return (
      <p className='font-mono text-[10px] text-destructive leading-relaxed'>
        {drill.error instanceof Error ? drill.error.message : "Failed to decompose the gap."}
      </p>
    );
  }

  if (!levels) {
    // No `levels` key = the engine returned None: either nothing beat the
    // significance gate, or this row is no longer in the scan. Both mean "no
    // decomposition", and neither is an empty chain — an empty chain would
    // read as "nothing wrong here" rather than "we could not decompose this".
    return (
      <p className='font-mono text-[10px] text-muted-foreground leading-relaxed'>
        Nothing to decompose here — either no split of this measure's gap could be told apart from
        sampling noise over this period, or this row is no longer in the current scan.
      </p>
    );
  }

  return (
    <DrillChain
      levels={levels}
      rootGap={drill.data?.root_gap ?? 0}
      rootUpside={drill.data?.root_upside ?? 0}
      idPrefix={root ? `${root.dimension}-${root.segment}` : "measure"}
    />
  );
}
