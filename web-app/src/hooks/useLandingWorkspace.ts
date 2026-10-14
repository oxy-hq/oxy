import { useWorkspacesCustomApps } from "@/hooks/api/customApps/useWorkspacesCustomApps";
import {
  navigableWorkspaces,
  pickWorkspace,
  rememberedWorkspace
} from "@/libs/utils/lastWorkspace";
import type { WorkspaceSummary } from "@/services/api/workspaces";

/**
 * The workspace a dispatcher lands an org on (`pickWorkspace`), with the app
 * counts that ordering needs. Counts are fetched only when there is a choice to
 * make — no remembered workspace and more than one navigable — so a returning
 * user's landing costs no extra request.
 *
 * `target` is null while `isPending`, and when nothing is navigable.
 */
export function useLandingWorkspace(
  workspaces: WorkspaceSummary[] | undefined,
  orgId: string | undefined
): { target: WorkspaceSummary | null; isPending: boolean } {
  const navigable = workspaces ? navigableWorkspaces(workspaces) : [];
  const remembered = orgId ? rememberedWorkspace(navigable, orgId) : null;
  const toCount = !remembered && navigable.length > 1 ? navigable.map((w) => w.id) : [];
  const { appsByWorkspace, isPending } = useWorkspacesCustomApps(toCount);

  if (!orgId || !workspaces || isPending) return { target: null, isPending };
  const counts = new Map([...appsByWorkspace].map(([id, apps]) => [id, apps.length]));
  return { target: pickWorkspace(workspaces, orgId, counts), isPending: false };
}
