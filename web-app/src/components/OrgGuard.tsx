import { useEffect } from "react";
import { Navigate, Outlet, useParams } from "react-router-dom";
import { Spinner } from "@/components/ui/shadcn/spinner";
import { useOrgs } from "@/hooks/api/organizations";
import { setLastOrgSlug } from "@/libs/utils/lastWorkspace";
import ROUTES from "@/libs/utils/routes";
import useCurrentOrg from "@/stores/useCurrentOrg";

/**
 * Route guard for /:orgSlug/* routes.
 * Resolves org from slug, verifies user is a member, sets Zustand store.
 * Redirects to / if org not found or user is not a member.
 */
export default function OrgGuard() {
  const { orgSlug } = useParams<{ orgSlug: string }>();
  const { data: orgs, isPending } = useOrgs();
  const { org: currentOrg, setOrg, clearOrg } = useCurrentOrg();

  const matchedOrg = orgs?.find((o) => o.slug === orgSlug);
  // Same trap applies if the matched org has no workspaces at all —
  // persisting it as lastOrgSlug would bounce the next PostLoginDispatcher
  // back into onboarding even if the user meant to visit another org.
  const hasWorkspaces = (matchedOrg?.workspace_count ?? 0) > 0;

  useEffect(() => {
    if (matchedOrg) {
      if (matchedOrg.id !== currentOrg?.id) {
        setOrg(matchedOrg);
      }
      if (hasWorkspaces) {
        setLastOrgSlug(matchedOrg.slug);
      }
    }
    // Clear stale org from store when orgs have loaded but slug is not found
    if (!isPending && !matchedOrg && currentOrg?.slug === orgSlug) {
      clearOrg();
    }
  }, [
    matchedOrg,
    currentOrg?.id,
    currentOrg?.slug,
    orgSlug,
    isPending,
    hasWorkspaces,
    setOrg,
    clearOrg
  ]);

  if (isPending) {
    return (
      <div className='flex h-full w-full items-center justify-center'>
        <Spinner className='size-6' />
      </div>
    );
  }

  if (!matchedOrg) {
    return <Navigate to={ROUTES.ROOT} replace />;
  }

  return <Outlet />;
}
