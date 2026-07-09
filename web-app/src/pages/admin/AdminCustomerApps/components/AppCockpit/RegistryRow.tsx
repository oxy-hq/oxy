import { forwardRef } from "react";
import { cn } from "@/libs/shadcn/utils";
import type { CustomerApp } from "@/types/apps";
import { AppHoverCard } from "../AppsTable/components/AppHoverCard";
import { StatusDot } from "../AppsTable/components/StatusDot";
import { formatRelativeTime } from "../AppsTable/useAppsTable";

interface RegistryRowProps {
  app: CustomerApp;
  selected: boolean;
  /** Show the org slug — hidden when the rail is grouped by org. */
  showOrg: boolean;
  onSelect: (app: CustomerApp) => void;
  onPublish: (app: CustomerApp) => void;
  onUnpublish: (app: CustomerApp) => void;
}

/**
 * One line in the registry rail — the cockpit's densest surface. A real button
 * (no nested interactive children here — the hover card's actions float in a
 * portal), so keyboard + a11y come for free. LED · name · source · recency in a
 * single 28px row; the selected row gets a brand wash + left accent bar so the
 * eye tracks it against the stage. `forwardRef` so the rail can scroll the
 * active row into view during ↑/↓ navigation.
 */
export const RegistryRow = forwardRef<HTMLButtonElement, RegistryRowProps>(
  ({ app, selected, showOrg, onSelect, onPublish, onUnpublish }, ref) => (
    <AppHoverCard app={app} showOrg={showOrg} onPublish={onPublish} onUnpublish={onUnpublish}>
      <button
        ref={ref}
        type='button'
        data-state={selected ? "selected" : undefined}
        aria-current={selected}
        onClick={() => onSelect(app)}
        className={cn(
          "group relative flex w-full items-center gap-2 rounded-md py-1.5 pr-2 pl-3 text-left outline-none transition-colors",
          "hover:bg-muted/60 focus-visible:ring-2 focus-visible:ring-ring",
          "data-[state=selected]:bg-primary/10"
        )}
      >
        {/* Left accent bar on the active row. */}
        <span
          aria-hidden
          className={cn(
            "absolute top-1.5 bottom-1.5 left-0 w-0.5 rounded-full bg-primary transition-opacity",
            selected ? "opacity-100" : "opacity-0"
          )}
        />
        <StatusDot isLive={!!app.published_at} />
        <span className='min-w-0 flex-1'>
          <span
            className={cn(
              "block truncate text-sm",
              selected ? "font-medium text-foreground" : "text-foreground/90"
            )}
          >
            {app.name}
          </span>
          {showOrg && (
            <span className='block truncate font-mono text-[10px] text-muted-foreground'>
              {app.org_slug}
            </span>
          )}
        </span>
        <span className='shrink-0 font-mono text-[10px] text-muted-foreground/70 uppercase'>
          {app.source_type}
        </span>
        <span className='w-8 shrink-0 text-right font-mono text-[10px] text-muted-foreground tabular-nums'>
          {formatRelativeTime(app.last_active_at ?? app.last_synced_at)}
        </span>
      </button>
    </AppHoverCard>
  )
);
RegistryRow.displayName = "RegistryRow";
