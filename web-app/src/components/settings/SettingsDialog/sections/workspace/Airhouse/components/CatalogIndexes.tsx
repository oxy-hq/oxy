import { Database, Loader2 } from "lucide-react";
import { Skeleton } from "@/components/ui/shadcn/skeleton";
import { Switch } from "@/components/ui/shadcn/switch";
import { useCatalogIndexes, useSetCatalogIndexes } from "@/hooks/api/airhouse/useCatalogIndexes";

type Props = {
  workspaceId: string;
};

/**
 * Toggle the tenant's DuckLake catalog hot-path indexes. Indexing the metadata
 * catalog speeds up concurrent dashboard scans; airhouse builds/drops the
 * indexes `CONCURRENTLY`, so the switch reflects an async `building → ready`
 * state — the hook polls the status while a build is in flight and the switch
 * locks with a spinner until it settles.
 *
 * Admin-only, and rendered only when the caller can manage: both catalog-index
 * routes are org-admin-gated server-side (the status GET included), so there is
 * no read-only view of this state to fall back to.
 */
export function CatalogIndexes({ workspaceId }: Props) {
  const { data: status, isPending, isError } = useCatalogIndexes(workspaceId);
  const set = useSetCatalogIndexes(workspaceId);

  // "ready" and "building" both mean the toggle is on; "absent" is off. On a
  // query error (airhouse down / no tenant) `status` is undefined — treat that
  // as unavailable, NOT off, so a broken backend doesn't masquerade as "off".
  const state = status?.state ?? "absent";
  const building = state === "building";
  const enabled = !isError && (state === "ready" || building);
  // A CONCURRENTLY build/drop is in flight, or a toggle request is in flight.
  const inFlight = building || set.isPending;

  return (
    <div className='flex items-start gap-4 rounded-lg border bg-card p-5'>
      <div className='flex size-10 shrink-0 items-center justify-center rounded-md border bg-primary/10 text-primary'>
        <Database className='size-5' />
      </div>

      <div className='flex flex-1 flex-col gap-1'>
        <p className='font-medium text-sm leading-tight'>Catalog indexes</p>
        {isPending ? (
          <Skeleton className='mt-1 h-3.5 w-64' />
        ) : (
          <p className='text-muted-foreground text-xs'>
            Index the DuckLake metadata catalog to speed up concurrent dashboard queries.{" "}
            {isError
              ? "Status unavailable — Airhouse didn't respond."
              : building
                ? "Building indexes…"
                : enabled
                  ? "On — catalog metadata indexed."
                  : "Off."}
          </p>
        )}
      </div>

      <div className='flex items-center gap-3'>
        {(inFlight || isPending) && (
          <Loader2 className='size-3.5 animate-spin text-muted-foreground' />
        )}
        <Switch
          checked={enabled}
          disabled={isPending || isError || inFlight}
          onCheckedChange={(next) => set.mutate(next)}
          aria-label='Toggle DuckLake catalog indexes for this workspace'
        />
      </div>
    </div>
  );
}
