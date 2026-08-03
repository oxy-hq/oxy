import { AppMark } from "@/components/apps/AppMark";
import { Badge } from "@/components/ui/shadcn/badge";
import { Checkbox } from "@/components/ui/shadcn/checkbox";
import { cn } from "@/libs/shadcn/utils";
import type { CustomApp } from "@/types/apps";
import { resolveBundleUrl } from "../../../resolveBundleUrl";
import { formatRelativeTime } from "../useAppsTable";
import { AppActionsMenu } from "./AppActionsMenu";
import { AppHoverCard } from "./AppHoverCard";
import { StatusDot } from "./StatusDot";
import { UrlLine } from "./UrlActions";

interface AppCardProps {
  app: CustomApp;
  showOrg: boolean;
  isSelected: boolean;
  onToggle: (shiftKey: boolean) => void;
  onOpen: (app: CustomApp) => void;
  onPublish: (app: CustomApp) => void;
  onUnpublish: (app: CustomApp) => void;
}

/**
 * Gallery card — pared to three text ranks so a grid of them stays scannable:
 *
 *   1. identity  — status LED + name (+ org when ungrouped)
 *   2. address   — one primary URL line (subdomain preferred)
 *   3. meta      — source badge · last-active, quiet
 *
 * The old card carried two URL lines, a status pill, and a promoter line on
 * top of that; those secondary facts now live in the hover card (which wraps
 * the whole tile), so the resting face is calm and hovering is the triage.
 *
 * The mark doubles as the selection target: hover/select swaps it for a
 * checkbox. The card opens the detail; checkbox, link, and ⋯ menu stop
 * propagation.
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
  <AppHoverCard app={app} showOrg={showOrg} onPublish={onPublish} onUnpublish={onUnpublish}>
    {/* biome-ignore lint/a11y/useSemanticElements: the card nests interactive
        controls (checkbox, link, menu), so a real <button> would be invalid
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
          <AppMark
            iconUrl={app.icon_url}
            name={app.name}
            size='sm'
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
          <span className='flex items-center gap-1.5'>
            <StatusDot isLive={!!app.published_at} />
            <span className='truncate font-medium text-foreground text-xs'>{app.name}</span>
          </span>
          {showOrg && (
            <span className='mt-0.5 block truncate font-mono text-muted-foreground text-xs'>
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

      <UrlLine href={app.url_subdomain ?? resolveBundleUrl(app.url)} copyLabel='app URL' />

      <div className='flex items-center gap-2 text-muted-foreground text-xs'>
        <Badge variant='outline' className='px-1.5 py-0 font-mono text-[10px] tracking-wide'>
          {app.source_type.toUpperCase()}
        </Badge>
        <span className='ml-auto tabular-nums'>
          {formatRelativeTime(app.last_active_at ?? app.last_synced_at)}
        </span>
      </div>
    </div>
  </AppHoverCard>
);
