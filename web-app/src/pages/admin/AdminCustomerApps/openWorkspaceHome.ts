import ROUTES from "@/libs/utils/routes";
import type { OxyAccessRow } from "@/types/apps";

/**
 * Open a granted workspace's main UI (/home) in a NEW TAB from the admin
 * browser. Opening a new tab keeps the operator in the access list and — unlike
 * the old same-tab navigate — doesn't seed/clobber their own session's
 * last-org / last-workspace hints just to peek at a tenant.
 *
 * Links straight to `WORKSPACE(id).HOME`, which pins the exact workspace that
 * granted access — no dispatcher heuristic, no org-root bounce. Requires the
 * backend workspace middleware to grant Global Owners/Admins access to
 * non-member orgs' workspaces (mirrors `org_middleware`).
 */
export function openWorkspaceHome(grant: OxyAccessRow): void {
  window.open(ROUTES.ORG(grant.org_slug).WORKSPACE(grant.workspace_id).HOME, "_blank", "noopener");
}
