import { Navigate, useParams } from "react-router-dom";
import { Spinner } from "@/components/ui/shadcn/spinner";
import { useOrgs } from "@/hooks/api/organizations";
import { useAllWorkspaces } from "@/hooks/api/workspaces/useWorkspaces";
import {
  clearLastWorkspaceId,
  pickWorkspace,
  setLastWorkspaceId
} from "@/libs/utils/lastWorkspace";
import ROUTES from "@/libs/utils/routes";

/**
 * Landing at `/:orgSlug`. OrgGuard has already verified the slug resolves to a
 * member org, so we just pick a workspace and redirect. Empty orgs go to
 * onboarding; if every workspace is still cloning, clear the stale
 * lastWorkspace id and send the user to onboarding to create a new one. Failed
 * workspaces are navigable (the workspace shell surfaces the error + retry),
 * so they count as pick targets.
 *
 * Resolves the org from the URL slug (not from `useCurrentOrg`) on purpose:
 * OrgGuard updates the Zustand store inside a useEffect, which fires *after*
 * this child renders. Reading from the store on the first render after an org
 * switch would return the *previous* org. With the previous org's workspaces
 * already hot in the React Query cache, `useAllWorkspaces` would return them
 * synchronously, `pickWorkspace` would pick the previous org's last workspace,
 * and the `<Navigate>` would bounce the user right back where they came from —
 * making the org switcher (or a direct URL change to /:newOrgSlug) appear to
 * silently do nothing.
 */
export default function OrgDispatcher() {
  const { orgSlug } = useParams<{ orgSlug: string }>();
  const { data: orgs, isPending: orgsPending } = useOrgs();
  const org = orgs?.find((o) => o.slug === orgSlug);
  const { data: workspaces, isPending: wsPending, isError } = useAllWorkspaces(org?.id);

  // Defensive: the route schema guarantees orgSlug, but useParams types it as
  // optional. Bail to root rather than spin forever on the false branch below.
  if (!orgSlug) return <Navigate to={ROUTES.ROOT} replace />;

  if (orgsPending || !org || wsPending) {
    return (
      <div className='flex h-full w-full items-center justify-center'>
        <Spinner className='size-6' />
      </div>
    );
  }

  if (isError) {
    return (
      <div className='flex h-full w-full items-center justify-center'>
        <p className='text-destructive text-sm'>Failed to load workspaces.</p>
      </div>
    );
  }

  if (!workspaces || workspaces.length === 0) {
    return <Navigate to={ROUTES.ORG(org.slug).ONBOARDING} replace />;
  }

  const target = pickWorkspace(workspaces, org.id);
  if (!target) {
    clearLastWorkspaceId(org.id);
    return <Navigate to={ROUTES.ORG(org.slug).ONBOARDING} replace />;
  }
  setLastWorkspaceId(org.id, target.id);

  return <Navigate to={ROUTES.ORG(org.slug).WORKSPACE(target.id).ROOT} replace />;
}
