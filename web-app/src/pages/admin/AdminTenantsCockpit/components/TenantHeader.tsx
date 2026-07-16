import { FolderTree, List } from "lucide-react";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/shadcn/tooltip";
import { cn } from "@/libs/shadcn/utils";
import { SWITCHER_TYPES, type TenantType, type TenantView } from "../useTenantSelection";

/**
 * The tenant surface header: a compact segmented entity switcher (the three
 * relationship-connected entities) plus a single icon toggle for the who-manages-
 * whom Map. No page chrome beyond this — the density budget goes to the data below.
 */
export default function TenantHeader({
  type,
  onTypeChange,
  view,
  onViewChange
}: {
  type: TenantType;
  onTypeChange: (t: TenantType) => void;
  view: TenantView;
  onViewChange: (v: TenantView) => void;
}) {
  return (
    <header className='flex h-11 shrink-0 items-center justify-between gap-3 border-b px-3'>
      <div className='flex items-center gap-3'>
        <span className='font-semibold text-sm'>Tenants</span>
        {/* The switcher only makes sense in list view. The map spans all three
            entities at once, so switching type there changes nothing — hide it
            rather than offer a no-op control. */}
        {view === "list" ? (
          <div className='flex items-center gap-0.5 rounded-md bg-muted/60 p-0.5'>
            {SWITCHER_TYPES.map((t) => (
              <button
                key={t.id}
                type='button'
                data-testid={`tenant-type-${t.id}`}
                onClick={() => onTypeChange(t.id)}
                className={cn(
                  "rounded px-2.5 py-1 font-medium text-xs transition-colors",
                  type === t.id
                    ? "bg-background text-foreground shadow-sm"
                    : "text-muted-foreground hover:text-foreground"
                )}
              >
                {t.label}
              </button>
            ))}
          </div>
        ) : (
          <span className='text-muted-foreground text-xs'>Relationship map</span>
        )}
      </div>

      <Tooltip>
        <TooltipTrigger asChild>
          <button
            type='button'
            data-testid={view === "map" ? "tenant-view-list" : "tenant-view-map"}
            onClick={() => onViewChange(view === "map" ? "list" : "map")}
            className={cn(
              "flex size-7 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-muted/60 hover:text-foreground",
              view === "map" && "bg-muted text-foreground"
            )}
          >
            {view === "map" ? <List className='size-4' /> : <FolderTree className='size-4' />}
          </button>
        </TooltipTrigger>
        <TooltipContent>{view === "map" ? "Back to list" : "Relationship map"}</TooltipContent>
      </Tooltip>
    </header>
  );
}
