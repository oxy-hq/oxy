import type { AirhouseFleetRow } from "@/services/api/airhouseAdmin";

/**
 * How wrong a tenant is, as one ordered value.
 *
 * The page exists to answer "is any tenant's data plane broken" — so severity is
 * the sort key, not the workspace name. A broken row is on screen before the
 * operator does anything, which is the only interaction budget an incident has.
 *
 * Kept as a pure module so the ordering and the counts can be tested without a
 * DOM: these are the page's actual claims, and asserting them through a rendered
 * table would test the table.
 */
export type Severity = "broken" | "degraded" | "healthy";

/** Highest first — this is the display order. */
export const SEVERITY_ORDER: Severity[] = ["broken", "degraded", "healthy"];

/**
 * `broken` is the silent one and outranks a loud `status`: a tenant with no
 * usable service account reads as provisioned everywhere else and cannot mint
 * the ephemeral credential a query needs, so it fails on first use with nothing
 * on this page having said so. A non-`active` status is at least already
 * visible as a status.
 */
export function severityOf(row: AirhouseFleetRow): Severity {
  if (row.status === "active" && !row.service_account_ready) return "broken";
  if (row.status !== "active") return "degraded";
  return "healthy";
}

/** What each severity is called in the filter, and what it means. */
export const SEVERITY_LABEL: Record<Severity, string> = {
  broken: "No service account",
  degraded: "Not active",
  healthy: "Healthy"
};

/**
 * Sort comparator: severity first, then workspace name so equal-severity rows
 * hold a stable, scannable order rather than whatever the server returned.
 */
export function bySeverityThenName(a: AirhouseFleetRow, b: AirhouseFleetRow): number {
  const rank = SEVERITY_ORDER.indexOf(severityOf(a)) - SEVERITY_ORDER.indexOf(severityOf(b));
  return rank !== 0 ? rank : a.workspace_name.localeCompare(b.workspace_name);
}

/** Count per severity, for the filter chips. */
export function countBySeverity(rows: AirhouseFleetRow[]): Record<Severity, number> {
  const counts: Record<Severity, number> = { broken: 0, degraded: 0, healthy: 0 };
  for (const row of rows) counts[severityOf(row)] += 1;
  return counts;
}
