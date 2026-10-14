import type { CustomAppSummary } from "@/types/apps";
import type { WorkspaceApps } from "../useOtherWorkspaceApps";
import { AppGrid } from "./AppGrid";

/**
 * HQ for a workspace with no apps of its own, when the org has apps elsewhere:
 * the same cards, grouped under the workspace each lives in. A card opens its app
 * by org and slug, so it links exactly as it would from its own workspace's HQ.
 */
export function OtherWorkspaceApps({
  groups,
  onManageAccess
}: {
  groups: WorkspaceApps[];
  onManageAccess?: (app: CustomAppSummary) => void;
}) {
  return (
    <div
      className='mx-auto flex w-full max-w-6xl flex-col gap-10 px-6 pt-12 pb-8'
      data-testid='launcher-other-workspace-apps'
    >
      {groups.map(({ workspace, apps }, i) => (
        <section key={workspace.id} className='flex flex-col gap-4'>
          <h2 className='font-medium text-muted-foreground text-sm'>Apps in {workspace.name}</h2>
          <AppGrid
            apps={apps}
            // Only the first group's first row is on screen at first paint.
            eagerCount={i === 0 ? 3 : 0}
            onManageAccess={onManageAccess}
            testId={`launcher-app-grid-${workspace.id}`}
          />
        </section>
      ))}
    </div>
  );
}
