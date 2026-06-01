import type { NavigateFunction } from "react-router-dom";
import { setLastOrgSlug, setLastWorkspaceId } from "@/libs/utils/lastWorkspace";
import ROUTES from "@/libs/utils/routes";
import type { OxyAccessGrant } from "@/types/apps";

/**
 * Jump into a granted workspace's main UI (/home) from the admin browser.
 *
 * We don't hand-hydrate the org/workspace stores — instead we seed the
 * dispatcher's "last org / last workspace" hints and navigate to the org
 * root, letting the normal `OrgGuard` dispatcher resolve the org and land on
 * this exact workspace. That reuses proven routing rather than duplicating
 * it. Caveat (accepted at design time): `OrgGuard` reads the
 * membership-scoped org list, so this only resolves when the admin is a
 * member of the org; otherwise it falls back to the admin's own default.
 */
export function openWorkspaceHome(grant: OxyAccessGrant, navigate: NavigateFunction): void {
  setLastOrgSlug(grant.org_slug);
  setLastWorkspaceId(grant.org_id, grant.workspace_id);
  navigate(ROUTES.ORG(grant.org_slug).ROOT);
}
