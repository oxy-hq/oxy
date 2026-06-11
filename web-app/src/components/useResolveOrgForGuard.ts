import { useAdminOrgsList } from "@/hooks/api/adminTenants/useAdminOrgs";
import { useOrgs } from "@/hooks/api/organizations";
import useCurrentUser from "@/hooks/api/users/useCurrentUser";
import type { AdminOrgMeta } from "@/services/api/adminTenants";
import type { Organization } from "@/types/organization";

export interface ResolvedOrg {
  org: Organization | undefined;
  /**
   * True when the org was resolved via the Global Owner / Global Admin
   * fallback below — i.e. the caller is NOT a real member. `OrgGuard` uses
   * this to skip the billing paywall (an operator inspecting a tenant
   * shouldn't be blocked by that tenant's subscription state).
   */
  isGlobalOverride: boolean;
  isPending: boolean;
}

/**
 * Resolve an org slug to an `Organization` for `OrgGuard`.
 *
 * Normal users: resolve against the membership-scoped org list (`/orgs`).
 *
 * Global Owners / Admins: when the slug isn't in their membership list (they
 * aren't a member of that tenant), fall back to the admin org directory
 * (`/admin/orgs-meta`, which returns every org for operators) and synthesize
 * an Owner-roled `Organization`. This is the client half of the "Open /home
 * on a workspace that granted Oxy access" fix — without it, `OrgGuard` bounces
 * operators to `/` before the workspace can load. The backend half grants the
 * matching workspace access (see `workspace_context::resolve_effective_role`).
 */
export function useResolveOrgForGuard(orgSlug: string | undefined): ResolvedOrg {
  const { data: orgs, isPending } = useOrgs();
  const member = orgs?.find((o) => o.slug === orgSlug);

  const { data: user } = useCurrentUser();
  const isOperator = Boolean(user?.is_owner || user?.is_app_admin);
  // Only reach for the admin directory once membership resolution has settled
  // and missed, and only for actual platform operators.
  const needFallback = Boolean(!isPending && !member && isOperator && orgSlug);

  const { data: adminOrgs, isPending: adminPending } = useAdminOrgsList(
    {},
    { enabled: needFallback }
  );
  const adminOrg = needFallback ? adminOrgs?.find((o) => o.slug === orgSlug) : undefined;

  if (member) {
    return { org: member, isGlobalOverride: false, isPending };
  }
  if (adminOrg) {
    return { org: toOrganization(adminOrg), isGlobalOverride: true, isPending: false };
  }
  return {
    org: undefined,
    isGlobalOverride: false,
    isPending: isPending || (needFallback && adminPending)
  };
}

function toOrganization(meta: AdminOrgMeta): Organization {
  return {
    id: meta.id,
    name: meta.name,
    slug: meta.slug,
    role: "owner",
    created_at: meta.created_at,
    workspace_count: meta.workspace_count,
    member_count: meta.member_count
  };
}
