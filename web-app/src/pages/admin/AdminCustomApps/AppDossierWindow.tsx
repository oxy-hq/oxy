import { AlertCircle } from "lucide-react";
import { useEffect } from "react";
import { useParams } from "react-router-dom";
import { Skeleton } from "@/components/ui/shadcn/skeleton";
import { DossierBody } from "./components/AppDetail/components/Dossier";
import { useAdminAppRegistry } from "./useAdminAppRegistry";

/**
 * The dossier as a standalone page, for the `window` dock mode.
 *
 * Mounted outside `AdminLayout` deliberately: this route only ever renders in a
 * popped-out window, where the admin nav chrome would be dead weight in a 560px
 * frame. The custom-app admin APIs it reads are app-admin gated server-side, so
 * an operator without standing gets empty sections rather than data.
 *
 * Its own route (rather than a React portal into `window.open`) so Radix
 * overlays inside the dossier — the rollback confirmation, copy tooltips —
 * portal into *this* window's document instead of the opener's.
 */
export default function AppDossierWindow() {
  const { orgSlug, appSlug } = useParams<{ orgSlug: string; appSlug: string }>();
  const { selected, isLoading, error } = useAdminAppRegistry(orgSlug, appSlug);

  useEffect(() => {
    if (selected) document.title = `${selected.name} · Details`;
  }, [selected]);

  return (
    <div className='flex h-screen min-h-0 flex-col bg-background'>
      <header className='flex h-9 shrink-0 items-center gap-2 border-b px-3'>
        {selected ? (
          <>
            <span className='truncate font-medium text-sm leading-none'>{selected.name}</span>
            <span className='min-w-0 truncate font-mono text-muted-foreground/70 text-xs'>
              {selected.org_slug}/{selected.slug}
            </span>
          </>
        ) : (
          <span className='font-mono text-[11px] text-muted-foreground uppercase tracking-wider'>
            Details
          </span>
        )}
      </header>

      {selected ? (
        <DossierBody app={selected} />
      ) : isLoading ? (
        <div className='space-y-3 p-4'>
          <Skeleton className='h-20 w-full' />
          <Skeleton className='h-40 w-full' />
        </div>
      ) : (
        <div className='flex items-center gap-2 p-4 text-destructive text-sm'>
          <AlertCircle className='size-4 shrink-0' />
          <span>
            {error
              ? "Couldn't load the app registry."
              : `No app matches ${orgSlug}/${appSlug}. It may have been deleted.`}
          </span>
        </div>
      )}
    </div>
  );
}
