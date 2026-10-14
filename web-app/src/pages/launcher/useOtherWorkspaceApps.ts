import { useWorkspacesCustomApps } from "@/hooks/api/customApps/useWorkspacesCustomApps";
import { useAllWorkspaces } from "@/hooks/api/workspaces/useWorkspaces";
import type { WorkspaceSummary } from "@/services/api/workspaces";
import type { CustomAppSummary } from "@/types/apps";

export interface WorkspaceApps {
  workspace: WorkspaceSummary;
  apps: CustomAppSummary[];
}

/**
 * For a workspace with no apps of its own: the apps this viewer can reach in the
 * org's other workspaces, one group per workspace that has any, in the org's
 * workspace order. A remembered workspace still wins the landing, so a user whose
 * last choice holds no apps would otherwise get an empty home while the org's
 * apps sit one switch away.
 *
 * Nothing is fetched while `enabled` is false — the common workspace, which has
 * apps, pays for no list but its own.
 */
export default function useOtherWorkspaceApps(
  currentWorkspaceId: string,
  enabled: boolean
): { groups: WorkspaceApps[]; isPending: boolean } {
  const { data: workspaces, isLoading } = useAllWorkspaces();
  const others = enabled ? (workspaces ?? []).filter((w) => w.id !== currentWorkspaceId) : [];
  const { appsByWorkspace, isPending } = useWorkspacesCustomApps(others.map((w) => w.id));
  const groups = others
    .map((workspace) => ({ workspace, apps: appsByWorkspace.get(workspace.id) ?? [] }))
    .filter((group) => group.apps.length > 0);
  // `isLoading`, not `isPending`: with no org (local mode) the list is disabled,
  // and a disabled query is pending forever.
  return { groups, isPending: enabled && (isLoading || isPending) };
}
