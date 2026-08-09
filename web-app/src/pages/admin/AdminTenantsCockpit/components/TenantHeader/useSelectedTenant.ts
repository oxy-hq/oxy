import { useQuery } from "@tanstack/react-query";
import { useDrainedAdminOrgs } from "@/hooks/api/adminTenants";
import queryKeys from "@/hooks/api/queryKey";
import type { AdminOrgMeta } from "@/services/api/adminTenants";
import { AuditService } from "@/services/api/audit";
import type { AuditEvent } from "@/types/audit";
import type { TenantType } from "../../useTenantSelection";

/**
 * Everything the header needs about the currently-selected tenant.
 *
 * Only orgs get the full treatment. Partners and users are selectable in the rail but
 * have no plan, no workspaces and no org-scoped audit trail, so a header that pretended
 * otherwise would show four empty slots — worse than showing nothing.
 *
 * The org list is already in the rail's cache, so reading the summary from it costs no
 * request. Recent activity is the one extra fetch, and it only fires for an org.
 */
export function useSelectedTenant(type: TenantType, id: string | null) {
  const isOrg = type === "orgs" && !!id;

  // Drained (capped at 50 it could not resolve the 51st org, leaving the header in its
  // browsing state for a tenant that is plainly selected) — but ONLY for orgs. Without
  // the gate, opening the Users or Partners tab fires the whole drain, page by page,
  // each page running five aggregate queries server-side, to compute an org that is
  // then discarded because `isOrg` is false. On the orgs tab the rail issues the same
  // query, so this is served from that cache.
  const { orgs } = useDrainedAdminOrgs({ enabled: type === "orgs" });
  const org: AdminOrgMeta | undefined = isOrg ? orgs.find((o) => o.id === id) : undefined;

  const params = { org_id: id ?? "", limit: 5 };
  const {
    data: activity = [],
    isPending: activityPending,
    isError: activityError
  } = useQuery<AuditEvent[]>({
    queryKey: queryKeys.audit.search(params),
    queryFn: () => AuditService.search(params),
    // Scoped to the selected org, so switching tenants switches the feed. Disabled for
    // non-orgs: `GET /admin/audit` without an org filter returns the whole platform's
    // history, which is emphatically not "recent activity for what I'm looking at".
    enabled: isOrg
  });

  return { isOrg, org, orgs, activity, activityPending, activityError };
}
