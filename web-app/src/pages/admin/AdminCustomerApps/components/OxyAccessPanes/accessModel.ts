import type { AdminOrgMeta } from "@/services/api/adminTenants";
import type { OxyAccessGrant } from "@/types/apps";

/**
 * One org in the Access cockpit, whether or not it granted Oxy access. Built by
 * joining the full org directory (admin meta) with the access grants, so the
 * surface can list orgs *without* access alongside those with it.
 */
export interface AccessOrg {
  orgId: string;
  orgName: string;
  orgSlug: string;
  /** Workspaces this org granted Oxy access to (empty when `hasAccess` is false). */
  grants: OxyAccessGrant[];
  hasAccess: boolean;
  /** Total workspaces in the org (from admin meta) — context for no-access orgs. */
  workspaceCount: number;
}

/**
 * Join every org (admin directory) with its access grants. Orgs with no grant
 * come back `hasAccess: false` with an empty `grants` list. A granted org that
 * somehow isn't in the admin directory (stale/racey meta) is still included via
 * union, so access is never silently hidden. Sorted by org name.
 */
export function buildAccessOrgs(orgs: AdminOrgMeta[], grants: OxyAccessGrant[]): AccessOrg[] {
  const byId = new Map<string, AccessOrg>();
  for (const o of orgs) {
    byId.set(o.id, {
      orgId: o.id,
      orgName: o.name,
      orgSlug: o.slug,
      grants: [],
      hasAccess: false,
      workspaceCount: o.workspace_count
    });
  }
  for (const g of grants) {
    const existing = byId.get(g.org_id) ?? {
      orgId: g.org_id,
      orgName: g.org_name,
      orgSlug: g.org_slug,
      grants: [],
      hasAccess: false,
      workspaceCount: 0
    };
    existing.grants.push(g);
    existing.hasAccess = true;
    byId.set(g.org_id, existing);
  }
  return [...byId.values()].sort((a, b) => a.orgName.localeCompare(b.orgName));
}
