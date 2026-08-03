import type { GrantablePerson, GranteeRef, GrantRole, Team } from "@/types/appAccess";
import type { GrantRow } from "./GrantList";

/**
 * Pure helpers for editing an app's access list.
 *
 * Split out of the dialog so the rules that actually matter — a grantee appears at
 * most once, the two kinds never collide, a team's headcount is what the grant
 * really reaches — are testable without mounting React.
 */

/**
 * Identity of a grantee across both kinds. A user and a team CAN share a UUID
 * (different tables), so the kind has to be part of the key.
 */
export const grantKey = (kind: "user" | "team", id: string): string => `${kind}:${id}`;

export const addGrant = (
  grants: GranteeRef[],
  kind: "user" | "team",
  id: string,
  role: GrantRole = "member"
): GranteeRef[] =>
  grants.some((g) => g.kind === kind && g.id === id) ? grants : [...grants, { kind, id, role }];

export const removeGrant = (
  grants: GranteeRef[],
  kind: "user" | "team",
  id: string
): GranteeRef[] => grants.filter((g) => !(g.kind === kind && g.id === id));

export const setGrantRole = (
  grants: GranteeRef[],
  kind: "user" | "team",
  id: string,
  role: GrantRole
): GranteeRef[] => grants.map((g) => (g.kind === kind && g.id === id ? { ...g, role } : g));

export const grantedKeys = (grants: GranteeRef[]): Set<string> =>
  new Set(grants.map((g) => grantKey(g.kind, g.id)));

/**
 * Join grants back to their display data.
 *
 * A grantee whose team or person has since disappeared still renders — as
 * "Unknown", not dropped. Silently hiding a row would mean saving the dialog
 * quietly deleted a grant the admin never saw.
 */
export const toGrantRows = (
  grants: GranteeRef[],
  teams: Team[],
  people: GrantablePerson[]
): GrantRow[] => {
  const teamById = new Map(teams.map((t) => [t.id, t]));
  const personById = new Map(people.map((p) => [p.user_id, p]));

  return grants.map((g) => {
    if (g.kind === "team") {
      const team = teamById.get(g.id);
      return {
        ...g,
        name: team?.name ?? "Unknown team",
        detail: team
          ? `${team.member_count} ${team.member_count === 1 ? "person" : "people"}`
          : null
      };
    }
    const person = personById.get(g.id);
    return {
      ...g,
      name: person?.name ?? "Unknown person",
      detail: person?.email ?? null
    };
  });
};

/**
 * Roughly how many people the current list reaches.
 *
 * Deliberately approximate: someone in two granted teams is counted twice. It
 * exists to catch "I just handed 40 people the admin surface", which it does
 * either way, and an exact figure would cost a round trip per edit.
 */
export const estimateReach = (grants: GranteeRef[], teams: Team[]): number => {
  const teamById = new Map(teams.map((t) => [t.id, t]));
  return grants.reduce(
    (sum, g) => sum + (g.kind === "team" ? (teamById.get(g.id)?.member_count ?? 0) : 1),
    0
  );
};
