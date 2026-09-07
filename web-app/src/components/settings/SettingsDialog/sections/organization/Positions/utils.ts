import type { AssignmentRow, RoleRow } from "@/types/operatingGraph";

/**
 * How many distinct people hold each position, by role id. One person at two
 * stores is two assignments but one holder — "held by 3 people" counts heads.
 */
export function holderCounts(assignments: AssignmentRow[]): Map<string, number> {
  const holders = new Map<string, Set<string>>();
  for (const a of assignments) {
    const set = holders.get(a.role_id);
    if (set) set.add(a.user_id);
    else holders.set(a.role_id, new Set([a.user_id]));
  }
  return new Map([...holders].map(([roleId, set]) => [roleId, set.size]));
}

/** "4 positions · 1 org-wide" — the section's one-line summary. */
export function positionSummary(roles: RoleRow[]): string {
  const total = roles.length;
  const orgWide = roles.filter((r) => r.scope === "franchisor").length;
  return `${total} ${total === 1 ? "position" : "positions"} · ${orgWide} org-wide`;
}

/** "3 people" / "1 person" / "Nobody" — the Held by cell. */
export function heldByLabel(count: number): string {
  if (count === 0) return "Nobody";
  return `${count} ${count === 1 ? "person" : "people"}`;
}
