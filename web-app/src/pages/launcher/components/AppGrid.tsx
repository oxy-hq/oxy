import type { CustomAppSummary } from "@/types/apps";
import { AppCard } from "./AppCard";

export function AppGrid({
  apps,
  eagerCount = 0,
  onManageAccess,
  testId
}: {
  apps: CustomAppSummary[];
  /**
   * How many leading cards load their art immediately. The launcher passes 3 for
   * the grid at the top of the page: the first row at the widest layout
   * (xl:grid-cols-3) is on screen at first paint, so its art should not wait for
   * layout; the rest stay lazy.
   */
  eagerCount?: number;
  onManageAccess?: (app: CustomAppSummary) => void;
  testId: string;
}) {
  return (
    <div className='grid grid-cols-1 gap-5 sm:grid-cols-2 xl:grid-cols-3' data-testid={testId}>
      {apps.map((app, i) => (
        // art_url in the key remounts the card when art changes, resetting its error fallback
        <AppCard
          key={`${app.id}:${app.art_url ?? ""}`}
          app={app}
          eagerArt={i < eagerCount}
          onManageAccess={onManageAccess}
        />
      ))}
    </div>
  );
}
