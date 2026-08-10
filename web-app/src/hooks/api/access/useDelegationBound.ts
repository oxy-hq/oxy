import { useMemo } from "react";
import useCurrentUser from "@/hooks/api/users/useCurrentUser";
import type { AppAdmin, PlatformRoleId } from "@/types/access";
import { useAppAdmins } from "./useAppAdmins";

/** What the current operator is allowed to issue. */
export interface DelegationBound {
  /** May they administer grants at all — `Cap::ManagePlatformGrants`. */
  canGrant: boolean;
  /**
   * The roles they may issue, weakest first. A Global Admin gets `["app_operator"]`;
   * only the Global Owner can mint a peer.
   */
  issuableRoles: PlatformRoleId[];
  /** Their own reach is unbounded, so any scope they issue is within it. */
  scopeAll: boolean;
  /** When bounded, the only orgs they may put inside a grant. */
  scopeOrgIds: string[];
  /** Their own row, when they hold one. Absent for the Global Owner — root has no row. */
  own?: AppAdmin;
}

/**
 * The client-side mirror of `oxy_authz::may_delegate` — **for shaping controls, not for
 * deciding access.** The server re-decides every write; this exists so the console offers
 * a role an operator can actually issue instead of one that 403s on submit.
 *
 * Derived from the grant list rather than a new endpoint, because that list already
 * returns every row *including the caller's own* — which is exactly `(role × scope)`, the
 * pair the bound compares. Adding the caller's standing to `GET /user` would have been a
 * second source for a fact already on the wire, and two sources drift.
 *
 * Matched by **email**, the grant table's natural key: a grant can be issued before the
 * person has ever signed in, so it is keyed by address rather than user id.
 *
 * The one asymmetry worth knowing: the Global Owner holds **no row**. Their standing is
 * the `OXY_OWNER` env allow-list, so `own` is undefined for them and every bound below
 * short-circuits on `is_owner` — the same shape `may_delegate` uses server-side.
 */
export function useDelegationBound(enabled = true): DelegationBound {
  const { data: user } = useCurrentUser();
  // Whether the caller may administer grants is answerable from `/user` alone, BEFORE
  // any list fetch — so gate the fetch on it. Passing a bare `enabled` here started the
  // request for everyone: `PlatformAccessCard` calls `useAppAdmins(bound.canGrant)`, but
  // that is the same query key, so react-query had already fired it and the card's own
  // "we don't request what would 403" reasoning was describing something that didn't
  // happen.
  const canGrant =
    (user?.is_owner ?? false) ||
    (user?.platform_capabilities ?? []).includes("manage_platform_grants");
  const { data: admins = [] } = useAppAdmins(enabled && canGrant);

  return useMemo(() => {
    if (user?.is_owner) {
      // Root: every role, every scope, no row.
      return {
        canGrant: true,
        issuableRoles: ["app_operator", "global_admin"],
        scopeAll: true,
        scopeOrgIds: []
      };
    }

    if (!canGrant) {
      return { canGrant: false, issuableRoles: [], scopeAll: false, scopeOrgIds: [] };
    }

    const own = admins.find((a) => a.email.toLowerCase() === user?.email?.toLowerCase());
    return {
      canGrant: true,
      // Strictly below the holder's own role. `global_admin` is the only role that
      // carries this capability today, so this is `["app_operator"]` — written as a
      // filter rather than a constant so adding a middle tier does not silently make
      // it wrong.
      issuableRoles: (["app_operator", "global_admin"] as PlatformRoleId[]).filter((r) =>
        own ? RANK[r] < RANK[own.role] : false
      ),
      scopeAll: own?.scope_all ?? false,
      scopeOrgIds: own?.scope_org_ids ?? [],
      own
    };
  }, [user, admins, canGrant]);
}

/** Mirrors `PlatformRole::rank`. Higher out-ranks lower. */
const RANK: Record<PlatformRoleId, number> = {
  app_operator: 1,
  global_admin: 2
};
