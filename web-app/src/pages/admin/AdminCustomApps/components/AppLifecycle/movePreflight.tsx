import type { useMovePlan } from "@/hooks/api/customApps/useProductionMove";
import { hardBlockers } from "@/hooks/api/customApps/useProductionMove";
import type { LifecyclePreflight } from "./LifecycleCard";
import { MovePlanSummary } from "./MovePlanSummary";

/**
 * What a production move's confirm says, and whether its verb waits: the dry-run
 * plan, and the one hold rule every surface that confirms a move uses — the
 * lifecycle card and the release timeline's Roll back alike.
 *
 * A plan being read or re-read, one whose last read failed, and one with a blocker
 * no confirm clears all hold the verb: Confirm would otherwise act on a plan the
 * server may have refused, or never showed.
 */
export function movePreflight(plan: ReturnType<typeof useMovePlan>): LifecyclePreflight {
  return {
    notice: <MovePlanSummary plan={plan.data} isLoading={plan.isLoading} error={plan.error} />,
    hold: plan.isFetching || !!plan.error || !plan.data || hardBlockers(plan.data).length > 0
  };
}
