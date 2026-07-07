import { Badge } from "@/components/ui/shadcn/badge";
import { Checkbox } from "@/components/ui/shadcn/checkbox";
import { cn } from "@/libs/shadcn/utils";
import type { CustomerApp } from "@/types/apps";
import { resolveBundleUrl } from "../../../resolveBundleUrl";
import { AppFavicon } from "../../AppFavicon";
import { formatRelativeTime } from "../useAppsTable";
import { AppActionsMenu, StatusPill } from "./AppActionsMenu";
import { UrlLine } from "./UrlActions";

interface AppCardProps {
  app: CustomerApp;
  showOrg: boolean;
  isSelected: boolean;
  onToggle: (shiftKey: boolean) => void;
  onOpen: (app: CustomerApp) => void;
  onPublish: (app: CustomerApp) => void;
  onUnpublish: (app: CustomerApp) => void;
}

/**
 * Gallery card — the deployment-platform pattern: favicon + name up top, the
 * URL(s) as clickable/copyable lines, a quiet meta row, and last-promoter
 * attribution. The favicon doubles as the selection target: hovering (or
 * selecting) swaps it for a checkbox, so an unselected grid stays calm. The
 * card opens the detail; the checkbox, links and ⋯ menu stop propagation.
 */
export const AppCard = ({
  app,
  showOrg,
  isSelected,
  onToggle,
  onOpen,
  onPublish,
  onUnpublish
}: AppCardProps) => (
  <>
    {/* biome-ignore lint/a11y/useSemanticElements: the card nests interactive
        controls (checkbox, links, menu), so a real <button> would be invalid
        button-in-button; a div with role/tabIndex reproduces the semantics. */}
    <div
      role='button'
      tabIndex={0}
      data-state={isSelected ? "selected" : undefined}
      onClick={() => onOpen(app)}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          onOpen(app);
        }
      }}
      className={cn(
        "group flex cursor-pointer flex-col gap-2.5 rounded-lg border border-border/60 bg-card p-3.5 text-left outline-none transition-colors",
        "hover:border-foreground/20 hover:bg-muted/20 focus-visible:ring-2 focus-visible:ring-ring",
        "data-[state=selected]:border-primary data-[state=selected]:bg-primary/5"
      )}
    >
      <div className='flex items-start gap-2.5'>
        <div className='relative flex size-5 shrink-0 items-center justify-center'>
          <AppFavicon
            app={app}
            className={cn("transition-opacity group-hover:opacity-0", isSelected && "opacity-0")}
          />
          <Checkbox
            checked={isSelected}
            onClick={(e) => {
              e.stopPropagation();
              onToggle(e.shiftKey);
            }}
            aria-label={`Select ${app.name}`}
            className={cn(
              "absolute opacity-0 transition-opacity focus-visible:opacity-100 group-hover:opacity-100",
              isSelected && "opacity-100"
            )}
          />
        </div>
        <div className='min-w-0 flex-1'>
          <span className='block truncate font-medium text-foreground text-sm'>{app.name}</span>
          {showOrg && (
            <span className='block truncate font-mono text-muted-foreground text-xs'>
              {app.org_slug}
            </span>
          )}
        </div>
        <AppActionsMenu
          app={app}
          onOpen={onOpen}
          onPublish={onPublish}
          onUnpublish={onUnpublish}
          triggerClassName='-mt-1 -mr-1'
        />
      </div>

      <div className='space-y-1'>
        <UrlLine href={resolveBundleUrl(app.url)} copyLabel='app URL' />
        {app.url_subdomain && (
          <UrlLine href={app.url_subdomain} label='sub' copyLabel='subdomain URL' />
        )}
      </div>

      <div className='flex items-center gap-2 text-muted-foreground text-xs'>
        <StatusPill isLive={!!app.published_at} />
        <span aria-hidden>·</span>
        <Badge variant='outline' className='px-1.5 py-0 font-mono text-[10px] tracking-wide'>
          {app.source_type.toUpperCase()}
        </Badge>
        <span className='ml-auto tabular-nums'>
          {formatRelativeTime(app.last_active_at ?? app.last_synced_at)}
        </span>
      </div>

      {app.last_promoted_by_email && (
        <p className='truncate text-muted-foreground/70 text-xs'>
          Promoted by {app.last_promoted_by_email}
          {app.last_promoted_at ? ` · ${formatRelativeTime(app.last_promoted_at)}` : ""}
        </p>
      )}
    </div>
  </>
);
