import { useMemo, useState } from "react";
import { useMetricTree } from "@/hooks/api/useMetricTree";
import type { OpportunityInstance } from "@/types/metricTree";
import type { WmSelection, WorldModel } from "@/types/worldModel";
import { declaringView } from "./measureTarget";
import {
  DEFAULT_PRESET_DAYS,
  WorldModelOpportunitiesSection
} from "./WorldModelOpportunitiesSection";

interface WorldModelInstanceOpportunityProps {
  measureName: string;
  /** True when the measure is promoted to this grain rather than declared on it. */
  induced: boolean;
  /** Source view of an induced measure — where it is actually declared. */
  promotedFrom?: string;
  /** `view` of the host entity, i.e. where a non-induced measure is declared. */
  entityView: string | undefined;
  /** `additive` | `non_additive` | `passthrough`. */
  additivity: string;
  /** The instance in focus — the scan is scoped to it. */
  instance: OpportunityInstance;
  /** The graph, for resolving a segment header back to a selectable node. */
  model: WorldModel;
  onSelect: (s: WmSelection) => void;
}

/**
 * Opportunity sizing for one computed measure listed under a selected instance.
 *
 * Scoped to the instance in focus, so it answers "within this store, where is
 * the upside?" — the question the surrounding panel is already asking, and the
 * one that matches the instance-scoped measure value shown directly above it.
 * It resolves the world-model `(entity, measure)` address to a metric-tree node
 * the same way `WorldModelMeasureAnalysis` does.
 *
 * The scope reshapes the answer, by design: dimensions the instance pins to a
 * single value (this store's own name, city, region) have no peers left to
 * benchmark against and drop out, leaving the cuts that still vary inside the
 * instance — order status, customer and shipping attributes. The population-wide
 * "which store lags?" question still has a home: it is what the measure panel
 * (`WorldModelMeasureAnalysis`) asks, unscoped.
 *
 * Unlike the measure panel it renders **nothing at all** — no spinner, no "no
 * node" copy — when the measure has no metric-tree node, so an instance's
 * measure list is not littered with empty toggles. It also passes
 * `hideWhenEmpty` so the section renders nothing when the sizing query comes
 * back with no data: an instance's measure list should never show an
 * "Opportunities" toggle that only opens onto a "no data" line. That resolution
 * is eager and costs one warehouse sizing scan per mounted measure (5-min
 * cached). Being scoped, that cache is per instance rather than shared across
 * them, so the cost is paid per instance visited.
 */
export function WorldModelInstanceOpportunity({
  measureName,
  induced,
  promotedFrom,
  entityView,
  additivity,
  instance,
  model,
  onSelect
}: WorldModelInstanceOpportunityProps) {
  const view = declaringView(induced, promotedFrom, entityView);
  const { data: tree } = useMetricTree();
  // Period preference is owned here so it survives the section resetting its
  // transient result state; the segment section requires it (an undefined
  // period would build an invalid date range and throw "Invalid time value").
  const [periodDays, setPeriodDays] = useState(DEFAULT_PRESET_DAYS);

  const node = useMemo(() => {
    if (!tree || !view) return undefined;
    return tree.nodes.find((n) => n.view === view && n.measure === measureName);
  }, [tree, view, measureName]);

  if (!node) return null;

  // Only additive sums and non-additive rates are sizable; passthrough is not.
  // But `sizable` alone is NOT the surviving gate after the two sections merged
  // into one. Every engine-accepted composite (e.g. `checks.net_revenue`,
  // `checks.gross_profit`) classifies as `passthrough` here while the node's own
  // `drillable` is true — the drill section used to reach those measures because
  // it mounted unconditionally and self-gated. With one section left, gating on
  // `sizable` alone would silently drop the affordance for exactly the composites
  // the decomposition exists to expose, so `drillable` is admitted alongside it.
  const sizable = additivity === "additive" || additivity === "non_additive";
  const drillEnabled = node.drillable || additivity === "additive";

  if (!sizable && !drillEnabled) return null;

  return (
    <WorldModelOpportunitiesSection
      nodeId={node.id}
      view={node.view}
      additivity={additivity}
      periodDays={periodDays}
      onPeriodDaysChange={setPeriodDays}
      instance={instance}
      model={model}
      onSelect={onSelect}
      hideWhenEmpty
    />
  );
}
