import type { PersonKind, RoleScope } from "@/types/operatingGraph";

/**
 * Labels the three org-settings sections (Locations, Positions, Crew) share
 * for one assignment, so "Clovis · Shift lead" reads the same everywhere.
 */

/** The place half of an assignment chip: the location's name, or what stands in for one. */
export function assignmentPlace(a: {
  role_scope: RoleScope;
  location_name: string | null;
}): string {
  if (a.role_scope === "franchisor") return "Org-wide";
  // A location-scoped position whose place has since been removed: say so
  // rather than promote it to org-wide, which would widen what it means.
  return a.location_name ?? "No location";
}

/** "Clovis · Shift lead", or "Org-wide · Area manager" for an org-wide position. */
export function assignmentLabel(a: {
  role_name: string;
  role_scope: RoleScope;
  location_name: string | null;
}): string {
  return `${assignmentPlace(a)} · ${a.role_name}`;
}

export const SCOPE_LABELS: Record<RoleScope, string> = {
  location: "At a location",
  franchisor: "Org-wide"
};

/** Short marker for a person who is crew, not a member; members carry no marker. */
export const PERSON_KIND_MARK: Record<PersonKind, string | null> = {
  member: null,
  frontline: "crew"
};
