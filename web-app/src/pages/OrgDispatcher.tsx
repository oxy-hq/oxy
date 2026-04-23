import { Navigate } from "react-router-dom";
import { Spinner } from "@/components/ui/shadcn/spinner";
import { useAllWorkspaces } from "@/hooks/api/workspaces/useWorkspaces";
import {
  clearLastWorkspaceId,
  pickWorkspace,
  setLastWorkspaceId
} from "@/libs/utils/lastWorkspace";
import ROUTES from "@/libs/utils/routes";
import useCurrentOrg from "@/stores/useCurrentOrg";

/**
 * Landing at `/:orgSlug`. OrgGuard has already resolved + stored the org, so
 * we just pick a workspace and redirect. Empty orgs go to onboarding; if
 * every workspace is still cloning, clear the stale lastWorkspace id and
 * send the user to onboarding to create a new one. Failed workspaces are
 * navigable (the workspace shell surfaces the error + retry), so they
 * count as pick targets.
 */
export default function OrgDispatcher() {
  const org = useCurrentOrg((s) => s.org);
  const { data: workspaces, isPending, isError } = useAllWorkspaces(org?.id);

  if (!org || isPending) {
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
