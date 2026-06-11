import type { NavigateFunction } from "react-router-dom";
import { setLastOrgSlug, setLastWorkspaceId } from "@/libs/utils/lastWorkspace";
import ROUTES from "@/libs/utils/routes";
import type { OxyAccessGrant } from "@/types/apps";

/**
 * Jump into a granted workspace's main UI (/home) from the admin browser.
 *
 * Navigate DIRECTLY to the workspace home rather than the org root. The old
 * "land on /:orgSlug and let OrgDispatcher pick a workspace" path had two
 * failure modes for an operator inspecting a tenant they don't belong to:
 *   1. `OrgGuard` resolved the slug only against the membership-scoped org
 *      list, so a non-member operator got bounced to `/` before anything
 *      loaded (now fixed: OrgGuard resolves orgs for Global Owners/Admins).
 *   2. The dispatcher's `pickWorkspace` heuristic could land on a *different*
 *      workspace than the one that granted access.
 * Going straight to `WORKSPACE(id).HOME` pins the exact workspace. We still
 * seed the last-org / last-workspace hints so a later dispatcher visit (e.g.
 * the org switcher) resolves the same way.
 *
 * Requires the backend workspace middleware to grant Global Owners/Admins
 * access to non-member orgs' workspaces (mirrors `org_middleware`).
 */
export function openWorkspaceHome(grant: OxyAccessGrant, navigate: NavigateFunction): void {
  setLastOrgSlug(grant.org_slug);
  setLastWorkspaceId(grant.org_id, grant.workspace_id);
  navigate(ROUTES.ORG(grant.org_slug).WORKSPACE(grant.workspace_id).HOME);
}
