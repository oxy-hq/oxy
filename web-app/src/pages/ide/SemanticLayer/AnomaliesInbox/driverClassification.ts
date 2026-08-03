/** What role a declared driver plays in an anomaly, and how to say so.
 *
 *  Three surfaces need this answer — the drawer's section grouping, the graph
 *  node's tone and subtitle, and the prompt context handed to the agent — and
 *  they must not disagree: a driver shown under "offsetting the move" while the
 *  agent is told it explains the move is worse than either alone. So the
 *  classification lives here once and the surfaces only render it. */
import type { DriverAttribution } from "@/types/metricTree";

import { shortMeasureName } from "@/utils/measureFormat";

/** A driver's role in the target's move.
 *
 *  - `contributing` — moved the way that pushes the target where it went.
 *  - `counteracting` — moved *against* it, so it dampened the anomaly rather
 *    than causing it.
 *  - `mechanical` — only moved because the base measure it tracks moved (see
 *    `PassthroughSplit`); says nothing about the target either way.
 *  - `unresolved` — no signed claim available. */
export type DriverRole = "contributing" | "counteracting" | "mechanical" | "unresolved";

/** Classify one driver.
 *
 *  `unresolved` is the fall-through rather than a match on `contribution ===
 *  "unknown"`, and that is load-bearing. These objects arrive from
 *  `explain_cache` rows that are served back verbatim with no schema version
 *  and no runtime validation, so `contribution` can legitimately hold a value
 *  this build's union does not list — an explain cached before the field
 *  shipped has it absent, and a later airlayer may add a fourth value. Matching
 *  on the known strings alone would put such a row in no group at all and drop
 *  it silently from the panel, which is the exact failure this classification
 *  was introduced to prevent. */
export function driverRole(driver: DriverAttribution): DriverRole {
  // Mechanical outranks the sign split: "it moved because its base moved" is
  // the dominant fact about such a driver, and calling it contributing or
  // offsetting implies an independence it does not have.
  if (driver.passthrough) return "mechanical";
  if (driver.contribution === "contributing") return "contributing";
  if (driver.contribution === "counteracting") return "counteracting";
  return "unresolved";
}

export interface GroupedDrivers {
  contributing: DriverAttribution[];
  counteracting: DriverAttribution[];
  mechanical: DriverAttribution[];
  unresolved: DriverAttribution[];
  /** True when something is unresolved because the explain predates
   *  classification (`contribution` absent) rather than because the classifier
   *  declined to make a call — the two want different advice, Refresh vs.
   *  declare a `direction` on the edge. */
  anyStale: boolean;
}

/** Partition drivers by role in one pass.
 *
 *  Indexing the accumulator by `driverRole`'s return value makes the grouping
 *  total by construction: there is no filter chain a driver can fall out of, so
 *  a row cannot go missing from the panel without also failing to typecheck. */
export function groupDrivers(drivers: DriverAttribution[]): GroupedDrivers {
  const grouped: GroupedDrivers = {
    contributing: [],
    counteracting: [],
    mechanical: [],
    unresolved: [],
    anyStale: false
  };
  for (const driver of drivers) {
    grouped[driverRole(driver)].push(driver);
  }
  // `== null` catches an explicit JSON `null` as well as an absent key: both mean
  // the field was never written, and the panel's advice differs from the
  // classifier's own "unknown" (hit Refresh vs. declare a `direction`).
  grouped.anyStale = grouped.unresolved.some((d) => d.contribution == null);
  return grouped;
}

/** A driver's role in the few words a graph-node subtitle allows. */
export function roleBadge(driver: DriverAttribution): string {
  if (driver.passthrough) return `tracks ${shortMeasureName(driver.passthrough.base_measure)}`;
  switch (driverRole(driver)) {
    case "contributing":
      return "explains";
    case "counteracting":
      return "offsets";
    default:
      return "direction undetermined";
  }
}

/** A driver's role spelled out for the agent, in words rather than the bare
 *  enum — "counteracting" on its own gets read as a weaker kind of "cause". */
export function roleInstruction(driver: DriverAttribution): string {
  switch (driverRole(driver)) {
    case "mechanical":
      return "moved mechanically with its base — NOT independent evidence, do not cite as a cause or an offset";
    case "contributing":
      return "explains part of the move";
    case "counteracting":
      return "moved AGAINST the anomaly — offset part of it, did NOT cause it";
    default:
      return "direction undetermined — cannot say which way it pushed";
  }
}

/** Which way a driver pushes the target, for tone: the same direction the
 *  target moved when contributing, the opposite when counteracting, and no
 *  claim otherwise.
 *
 *  Never the sign of the driver's *own* delta — a `direction: negative` driver
 *  that fell pushed the target *up*, and toning that node with the target's own
 *  down-tone is what made an offsetting discount read as a cause of the drop.
 *  `unresolved` and `mechanical` stay neutral: an absent classification must
 *  not default into the target's tone, which is that same misread. */
export function driverPush(driver: DriverAttribution, targetDelta: number): number | null {
  switch (driverRole(driver)) {
    case "contributing":
      return targetDelta;
    case "counteracting":
      return -targetDelta;
    default:
      return null;
  }
}
