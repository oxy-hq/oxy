import { useMemo } from "react";
import { Navigate } from "react-router-dom";
import { Spinner } from "@/components/ui/shadcn/spinner";
import { useOrgs } from "@/hooks/api/organizations";
import { useAllWorkspaces } from "@/hooks/api/workspaces/useWorkspaces";
import {
  clearLastWorkspaceId,
  getLastOrgSlug,
  pickWorkspace,
  setLastWorkspaceId
} from "@/libs/utils/lastWorkspace";
import ROUTES from "@/libs/utils/routes";
import type { Organization } from "@/types/organization";

/**
 * Landing component at `/`. Runs the "where should this user go?" routine
 * that cannot fit in the synchronous `handlePostLoginOrgs` because it needs
 * to fetch workspaces.
 *
 *   0 orgs                        → /onboarding
 *   has orgs, 0 workspaces        → /:slug/onboarding
 *   has orgs, no navigable ws     → /:slug/onboarding (every ws is still cloning)
 *   has orgs, ≥1 navigable ws     → /:slug/workspaces/:last-or-first-navigable
 *
 * The chosen org follows (a) last-org-slug from localStorage, else (b) the
 * first org returned by the API. Navigable means `status === "ready"` or
 * `"failed"` — cloning is skipped because it's transient, but failed is kept
 * so the user lands on the actual last workspace and can retry from there
 * instead of being silently routed away.
 */
export default function PostLoginDispatcher() {
  const { data: orgs, isPending: orgsPending, isError: orgsError } = useOrgs();

  const chosenOrg = useMemo(() => pickOrg(orgs), [orgs]);

  // Pass chosenOrg.id explicitly — the dispatcher runs at `/` before any
  // OrgGuard has primed the store, so `useAllWorkspaces`'s store fallback
  // would otherwise either be empty or carry a value from a prior org.
  const {
    data: workspaces,
    isPending: wsPending,
    isError: wsError
  } = useAllWorkspaces(chosenOrg?.id);

  if (orgsPending) return <FullPageSpinner />;

  if (orgsError) {
    return (
      <div className='flex h-full w-full items-center justify-center'>
        <p className='text-destructive text-sm'>Failed to load organizations.</p>
      </div>
    );
  }

  if (!orgs || orgs.length === 0) {
    return <Navigate to={ROUTES.ONBOARDING} replace />;
  }

  if (!chosenOrg) return <FullPageSpinner />;

  if (wsPending) return <FullPageSpinner />;

  if (wsError) {
    // Fail open: send the user to the org root so the org dispatcher can retry.
    return <Navigate to={ROUTES.ORG(chosenOrg.slug).ROOT} replace />;
  }

  if (!workspaces || workspaces.length === 0) {
    return <Navigate to={ROUTES.ORG(chosenOrg.slug).ONBOARDING} replace />;
  }

  const target = pickWorkspace(workspaces, chosenOrg.id);
  if (!target) {
    // Every workspace is still cloning or failed — send to onboarding so the
    // user can create a new one. Drop any stale per-org lastWorkspace id so
    // next visit doesn't re-select the broken workspace.
    clearLastWorkspaceId(chosenOrg.id);
    return <Navigate to={ROUTES.ORG(chosenOrg.slug).ONBOARDING} replace />;
  }
  setLastWorkspaceId(chosenOrg.id, target.id);

  return <Navigate to={ROUTES.ORG(chosenOrg.slug).WORKSPACE(target.id).ROOT} replace />;
}

function FullPageSpinner() {
  return (
    <div className='flex h-full w-full items-center justify-center'>
      <Spinner className='size-6' />
    </div>
  );
}

function pickOrg(orgs: Organization[] | undefined): Organization | null {
  if (!orgs || orgs.length === 0) return null;

  const lastSlug = getLastOrgSlug();
  if (lastSlug) {
    const byLastSlug = orgs.find((o) => o.slug === lastSlug);
    if (byLastSlug) return byLastSlug;
  }

  return orgs[0];
}
