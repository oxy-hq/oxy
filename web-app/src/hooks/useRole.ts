import { useMemo } from "react";

import useCurrentOrg from "@/stores/useCurrentOrg";
import useCurrentWorkspace from "@/stores/useCurrentWorkspace";
import type { OrgRole, WorkspaceRole } from "@/types/organization";

/**
 * Single source of truth for role-based UI gating. Mirrors the backend's
 * typed role guards (OrgOwner, OrgAdmin, WorkspaceAdmin, WorkspaceEditor)
 * so FE and BE use the same vocabulary.
 *
 * `org` comes from the active org in the sidebar switcher.
 * `workspace` comes from the loaded workspace's `current_user_role`
 * (populated by `GET /workspaces/:id/details`). It is `undefined` until the
 * workspace details have loaded — check the specific boolean you need
 * instead of branching on the raw role.
 */
export function useRole() {
  const orgRole = useCurrentOrg((s) => s.role);
  const wsRole = useCurrentWorkspace((s) => s.workspace?.current_user_role);

  return useMemo(() => roleState(orgRole, wsRole), [orgRole, wsRole]);
}

function roleState(org: OrgRole | null, workspace: WorkspaceRole | undefined) {
  const isOrgOwner = org === "owner";
  const isOrgAdmin = org === "owner" || org === "admin";
  const isWorkspaceAdmin = workspace === "owner" || workspace === "admin";
  const isWorkspaceEditor = workspace !== undefined && workspace !== "viewer";

  return {
    org,
    workspace,
    is: {
      orgOwner: isOrgOwner,
      orgAdmin: isOrgAdmin,
      workspaceAdmin: isWorkspaceAdmin,
      workspaceEditor: isWorkspaceEditor
    }
  };
}

export type UseRoleResult = ReturnType<typeof useRole>;
