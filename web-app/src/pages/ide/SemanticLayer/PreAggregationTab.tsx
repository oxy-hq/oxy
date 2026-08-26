import { Loader2, RefreshCw } from "lucide-react";
import { useCallback, useMemo, useState } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/shadcn/button";
import { Input } from "@/components/ui/shadcn/input";
import { Skeleton } from "@/components/ui/shadcn/skeleton";
import { Table, TableBody, TableHead, TableHeader, TableRow } from "@/components/ui/shadcn/table";
import usePreaggStatus from "@/hooks/api/usePreaggStatus";
import useRebuildPreagg from "@/hooks/api/useRebuildPreagg";
import type { PreaggRollupStatus } from "@/services/api/semantic";
import RollupRow from "./components/preagg/RollupRow";
import { usePendingRebuilds } from "./components/preagg/usePendingRebuilds";

const matches = (rollup: PreaggRollupStatus, q: string) =>
  rollup.view_name.toLowerCase().includes(q) ||
  rollup.rollup_name.toLowerCase().includes(q) ||
  rollup.dimensions.some((d) => d.toLowerCase().includes(q)) ||
  rollup.measures.some((m) => m.name.toLowerCase().includes(q));

/**
 * Pre-aggregation tab: every rollup the semantic layer DECLARES, and whether
 * it has been built.
 *
 * The list is config, not cache — a workspace that has built nothing still
 * shows what it means to build, with every row reading "Not built". Deriving
 * the list from the manifest instead made the tab a view of what happened to
 * run, which answered the wrong question.
 */
export default function PreAggregationTab() {
  const [filter, setFilter] = useState("");
  const [pollMs, setPollMs] = useState<number | undefined>(undefined);
  const { data, isLoading, isError } = usePreaggStatus({ refetchIntervalMs: pollMs });
  const rebuild = useRebuildPreagg();

  const all = data?.rollups ?? [];
  // Whether a rollup built on another node can be read from here at all. The
  // status vocabulary depends on it: without shared storage, "built elsewhere"
  // means the warehouse answers, and saying otherwise would promise a fast
  // path this deployment doesn't have.
  const blobReads = data?.blob_reads_available ?? false;
  const rollups = useMemo(() => {
    const q = filter.trim().toLowerCase();
    const matched = q ? all.filter((r) => matches(r, q)) : all;
    // Group visually by view without a second render path: sort, don't bucket.
    return [...matched].sort(
      (a, b) => a.view_name.localeCompare(b.view_name) || a.rollup_name.localeCompare(b.rollup_name)
    );
  }, [all, filter]);

  // Counts BUILT, not locally-cached: a rollup another node built is serving
  // queries just as much as one on this disk, so "3 of 5 built" would be a lie
  // if it only counted local files.
  const builtCount = rollups.filter((r) => r.is_built).length;

  // A rebuild that fails never touches the manifest, so the row would just
  // stop spinning with nothing to show for it. Say so, and point at the run.
  const onGiveUp = useCallback((keys: string[]) => {
    toast.error(
      keys.length === 1
        ? `${keys[0]} didn't finish rebuilding — check the run history.`
        : `${keys.length} rollups didn't finish rebuilding — check the run history.`
    );
  }, []);
  const pending = usePendingRebuilds(all, onGiveUp);
  // Poll only while something is in flight — this endpoint reads the IDE
  // node's disk, so an always-on interval would be a standing cost for a
  // screen that is usually just being looked at.
  if (pollMs !== pending.pollMs) setPollMs(pending.pollMs);

  /** Rebuild one rollup, or everything the current filter is showing. */
  const startRebuild = (targets: PreaggRollupStatus[], all: boolean) => {
    // Optimistic: the spinner appears on the click, not a round-trip later.
    // If the submit itself fails the rows are released immediately — the
    // mutation's own onError raises the toast, so leaving them spinning would
    // contradict it for the next five minutes and then blame a run that was
    // never created.
    pending.markPending(targets);
    rebuild.mutate(all ? {} : { view: targets[0].view_name, rollup: targets[0].rollup_name }, {
      onError: () => pending.clearPending(targets)
    });
  };

  return (
    <div className='flex h-full min-h-0 flex-col' data-testid='pre-aggregation-tab'>
      <div className='flex items-center gap-3 border-border border-b px-4 py-2.5'>
        <Input
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
          placeholder='Filter rollups…'
          className='h-8 max-w-64'
          aria-label='Filter rollups'
        />
        {/* Counts what the table below shows, so it agrees with the rows the
            filter left standing. Hidden when there are none — "0 of 0 built"
            above an empty state says nothing. */}
        {!isLoading && !isError && rollups.length > 0 && (
          <span className='text-muted-foreground text-sm'>
            {builtCount} of {rollups.length} built
            {rollups.length !== all.length && ` (${all.length} total)`}
          </span>
        )}
        {!isLoading && !isError && all.length > 0 && (
          <Button
            variant='outline'
            size='sm'
            className='ml-auto h-8'
            disabled={rebuild.isPending || pending.anyPending}
            // Rebuild-all is the whole declared set, not the filtered view —
            // the server takes "everything" as its own case, and a button whose
            // scope silently followed a text box would be a nasty surprise at
            // 30 rollups.
            onClick={() => startRebuild(all, true)}
          >
            {pending.anyPending ? (
              <Loader2 className='h-3.5 w-3.5 animate-spin' />
            ) : (
              <RefreshCw className='h-3.5 w-3.5' />
            )}
            Rebuild all
          </Button>
        )}
      </div>

      <div className='min-h-0 flex-1 overflow-auto'>
        {isLoading ? (
          <div className='space-y-2 p-4'>
            <Skeleton className='h-8 w-full' />
            <Skeleton className='h-8 w-full' />
            <Skeleton className='h-8 w-full' />
          </div>
        ) : isError ? (
          <p className='p-4 text-muted-foreground text-sm'>
            Could not read the pre-aggregation cache status.
          </p>
        ) : rollups.length === 0 ? (
          // Now that the list is config-derived, an empty one has a single
          // meaning — the layer declares no rollups — and can be said plainly.
          <p className='p-4 text-muted-foreground text-sm' data-testid='pre-aggregation-empty'>
            {all.length > 0
              ? "No rollup matches this filter."
              : "No pre-aggregations declared. Add a `pre_aggregations:` block to a view to cache it locally."}
          </p>
        ) : (
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>View</TableHead>
                <TableHead>Rollup</TableHead>
                <TableHead>Status</TableHead>
                <TableHead>Dimensions</TableHead>
                <TableHead>Measures</TableHead>
                <TableHead>Time</TableHead>
                <TableHead>Refresh</TableHead>
                <TableHead>Built</TableHead>
                <TableHead className='sr-only'>Actions</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {rollups.map((rollup) => (
                <RollupRow
                  key={`${rollup.view_name}.${rollup.rollup_name}`}
                  rollup={rollup}
                  blobReads={blobReads}
                  rebuilding={pending.isPending(rollup)}
                  onRebuild={() => startRebuild([rollup], false)}
                />
              ))}
            </TableBody>
          </Table>
        )}
      </div>
    </div>
  );
}
