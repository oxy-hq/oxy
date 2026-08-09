import { useEffect, useMemo } from "react";
import type { AdminOrgMeta } from "@/services/api/adminTenants";
import { useAllAdminOrgs } from "./useAdminOrgs";

/**
 * Every org on the deployment, all pages drained.
 *
 * `useAdminOrgsList({})` sends no page params and `/admin/orgs-meta` defaults to
 * **50, ordered by name** — so anything built on it silently answers "the orgs
 * alphabetically early enough". That is tolerable for a search box (you typed a name and
 * got nothing, which reads as absent) and wrong for anything that makes a
 * deployment-wide claim.
 *
 * Four call sites had reached for the capped list before this existed: the tenant rail's
 * Managed / Direct / **Empty** chips, the header's selected-org lookup, the
 * assign-to-org picker, and the grant-scope picker. "Empty — provisioned but never used"
 * is the clearest case: its entire value is exhaustiveness, and an operator hunting
 * abandoned tenants past the 50th would be told there are none.
 *
 * The drain loop was copy-pasted in `AccessPane` and nowhere else; this is that loop,
 * extracted, so a fifth caller gets it by default instead of by remembering.
 */
export function useDrainedAdminOrgs({ enabled = true }: { enabled?: boolean } = {}): {
  orgs: AdminOrgMeta[];
  /** No page has arrived yet — nothing to show. */
  isLoading: boolean;
  /** Pages are still arriving: `orgs` is real but incomplete. */
  isDraining: boolean;
} {
  const { data, isLoading, hasNextPage, isFetchingNextPage, fetchNextPage } =
    useAllAdminOrgs(enabled);

  useEffect(() => {
    // Gated on `enabled` too: a disabled query reports `hasNextPage: false`, but a
    // caller that flips enabled mid-render should start draining, not sit on page one.
    if (enabled && hasNextPage && !isFetchingNextPage) fetchNextPage();
  }, [enabled, hasNextPage, isFetchingNextPage, fetchNextPage]);

  const orgs = useMemo(() => data?.pages.flat() ?? [], [data]);
  // Two states, not one. Collapsing them into `isLoading` meant a large deployment saw
  // a skeleton until the LAST page landed, where before the first 50 painted at once —
  // a first-paint regression for everyone who just wanted to type a name. Callers show
  // `orgs` as soon as `isLoading` clears, and use `isDraining` to disable the controls
  // whose correctness depends on the full set (the Empty / Managed / Direct chips).
  return { orgs, isLoading, isDraining: hasNextPage === true };
}
