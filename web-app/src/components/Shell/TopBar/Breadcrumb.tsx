import { Link } from "react-router-dom";
import ROUTES from "@/libs/utils/routes";
import useCurrentOrg from "@/stores/useCurrentOrg";
import useCurrentWorkspace from "@/stores/useCurrentWorkspace";
import { usePageName } from "./usePageName";

/**
 * "<Workspace> / <Page>" — e.g. "Poke House / HQ". The workspace name is a
 * link back to the HQ home. Page identity lives here, replacing the launcher's
 * removed HQ heading + status line. The workspace logo is intentionally NOT
 * shown — the rail's workspace tile already carries it, so a second copy here
 * is redundant.
 */
export function Breadcrumb() {
  const orgName = useCurrentOrg((s) => s.org?.name);
  const orgSlug = useCurrentOrg((s) => s.org?.slug) ?? "";
  const { workspace } = useCurrentWorkspace();
  const wsId = workspace?.id ?? "";
  const ws = ROUTES.ORG(orgSlug).WORKSPACE(wsId);
  const pageName = usePageName();

  // Org name in cloud; workspace name as a fallback; plain label in local mode.
  const workspaceName = orgName || workspace?.name || "Workspace";

  return (
    <div className='flex min-w-0 items-center gap-2 text-sm'>
      <Link
        to={ws.HOME}
        data-testid='topbar-workspace-link'
        className='shrink-0 font-medium text-foreground transition-opacity hover:opacity-70'
      >
        {workspaceName}
      </Link>
      <span className='text-muted-foreground/40'>/</span>
      <span className='truncate text-muted-foreground'>{pageName}</span>
    </div>
  );
}
