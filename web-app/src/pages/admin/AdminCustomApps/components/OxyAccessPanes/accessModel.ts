import type { AdminOrgMeta } from "@/services/api/adminTenants";
import type { OxyAccessRow } from "@/types/apps";

/**
 * One org in the Access browser. Oxy staff can reach every workspace BY DEFAULT
 * (inverted 2026-07-14) — so the interesting signal is no longer "did they grant
 * us access?" but "have they locked us OUT of anything?".
 */
export interface AccessOrg {
  orgId: string;
  orgName: string;
  orgSlug: string;
  /** Every workspace in the org, each carrying its lockdown state. */
  workspaces: OxyAccessRow[];
  accessibleCount: number;
  lockedCount: number;
  /** True when at least one workspace has locked Oxy out — the exception to surface. */
  hasLockdown: boolean;
  /** Total workspaces (from admin meta; falls back to the access rows). */
  workspaceCount: number;
}

/**
 * Join every org (admin directory) with its workspaces' lockdown rows. An org in
 * the access rows but missing from the admin directory (stale/racey meta) is
 * still included via union, so a lockdown is never silently hidden. Sorted:
 * locked-out orgs first (they're what an operator is hunting for), then by name.
 */
export function buildAccessOrgs(orgs: AdminOrgMeta[], rows: OxyAccessRow[]): AccessOrg[] {
  const byId = new Map<string, AccessOrg>();
  for (const o of orgs) {
    byId.set(o.id, {
      orgId: o.id,
      orgName: o.name,
      orgSlug: o.slug,
      workspaces: [],
      accessibleCount: 0,
      lockedCount: 0,
      hasLockdown: false,
      workspaceCount: o.workspace_count
    });
  }
  for (const r of rows) {
    const existing = byId.get(r.org_id) ?? {
      orgId: r.org_id,
      orgName: r.org_name,
      orgSlug: r.org_slug,
      workspaces: [],
      accessibleCount: 0,
      lockedCount: 0,
      hasLockdown: false,
      workspaceCount: 0
    };
    existing.workspaces.push(r);
    if (r.locked) {
      existing.lockedCount += 1;
      existing.hasLockdown = true;
    } else {
      existing.accessibleCount += 1;
    }
    byId.set(r.org_id, existing);
  }
  return [...byId.values()].sort(
    (a, b) => Number(b.hasLockdown) - Number(a.hasLockdown) || a.orgName.localeCompare(b.orgName)
  );
}
