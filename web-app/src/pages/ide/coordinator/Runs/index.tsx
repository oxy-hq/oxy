import { ListChecks, RefreshCw } from "lucide-react";
import type React from "react";
import { useState } from "react";
import { useSearchParams } from "react-router-dom";
import { toast } from "sonner";
import { Button } from "@/components/ui/shadcn/button";
import { Checkbox } from "@/components/ui/shadcn/checkbox";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow
} from "@/components/ui/shadcn/table";
import useCoordinatorLive from "@/hooks/api/coordinator/useCoordinatorLive";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { useRole } from "@/hooks/useRole";
import { AnalyticsService } from "@/services/api/analytics";
import { CoordinatorService } from "@/services/api/coordinator";
import { ErrorState, LoadingState } from "../components/PageState";
import { RunRow } from "./components/RunRow";
import { RunsBulkBar } from "./components/RunsBulkBar";
import { DEFAULT_RUN_FILTERS, type RunFilters, RunsFilterBar } from "./components/RunsFilterBar";
import { useRunsModel } from "./useRunsModel";

const PAGE = 50;

/** Read filters from the URL so a filtered run view is shareable. */
const readFilters = (p: URLSearchParams): RunFilters => ({
  status: p.get("status") ?? "all",
  type: p.get("type") ?? "all",
  source: p.get("source") ?? "all",
  range: p.get("range") ?? DEFAULT_RUN_FILTERS.range,
  search: p.get("search") ?? "",
  includeSystem: p.get("system") === "1"
});

/**
 * Runs — the log of executions. Answers "what failed in the last hour?".
 * Filters live in the URL; the whole row drills into the run detail.
 */
const RunsPage: React.FC = () => {
  const [params, setParams] = useSearchParams();
  const filters = readFilters(params);
  const [limit, setLimit] = useState(PAGE);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [busy, setBusy] = useState(false);

  const { project } = useCurrentProjectBranch();
  const canManage = useRole().is.workspaceAdmin;
  const { runs, hasMore, isPending, error, refetch } = useRunsModel(filters, limit);
  useCoordinatorLive();

  const setFilters = (next: RunFilters) => {
    const p = new URLSearchParams();
    for (const [k, v] of Object.entries(next)) {
      if (k === "includeSystem") continue;
      if (typeof v !== "string") continue;
      if (v && v !== "all" && !(k === "range" && v === DEFAULT_RUN_FILTERS.range)) p.set(k, v);
    }
    if (next.includeSystem) p.set("system", "1");
    setParams(p, { replace: true });
    setSelected(new Set());
  };

  const toggle = (runId: string) =>
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(runId)) next.delete(runId);
      else next.add(runId);
      return next;
    });

  const allSelected = runs.length > 0 && runs.every((r) => selected.has(r.runId));
  const toggleAll = () => setSelected(allSelected ? new Set() : new Set(runs.map((r) => r.runId)));

  const selectedRuns = runs.filter((r) => selected.has(r.runId));
  const cancellableRuns = selectedRuns.filter((r) => r.live);
  const retryableRuns = selectedRuns.filter(
    (r) => r.status === "failed" || r.status === "cancelled"
  );

  const cancelSelected = async () => {
    if (cancellableRuns.length === 0) {
      toast.error("Selected runs are already settled");
      return;
    }
    setBusy(true);
    let ok = 0;
    for (const r of cancellableRuns) {
      try {
        await AnalyticsService.cancelRun(project.id, r.runId);
        ok++;
      } catch (e) {
        console.error("Failed to cancel run", r.runId, e);
      }
    }
    setBusy(false);
    setSelected(new Set());
    if (ok > 0) {
      toast.success(`Cancelled ${ok} run${ok === 1 ? "" : "s"}`);
      refetch();
    }
    if (ok < cancellableRuns.length)
      toast.error(`${cancellableRuns.length - ok} run(s) could not be cancelled`);
  };

  const retrySelected = async () => {
    if (retryableRuns.length === 0) {
      toast.error("Only failed or cancelled runs can be retried");
      return;
    }
    setBusy(true);
    let ok = 0;
    for (const r of retryableRuns) {
      try {
        await CoordinatorService.retryRun(project.id, r.runId);
        ok++;
      } catch (e) {
        console.error("Failed to retry run", r.runId, e);
      }
    }
    setBusy(false);
    setSelected(new Set());
    if (ok > 0) {
      toast.success(`Retried ${ok} run${ok === 1 ? "" : "s"}`);
      refetch();
    }
    if (ok < retryableRuns.length)
      toast.error(`${retryableRuns.length - ok} run(s) could not be retried`);
  };

  return (
    <div className='flex h-full flex-col'>
      <div className='flex items-center justify-between border-border border-b px-4 py-2.5'>
        <div>
          <h2 className='font-semibold text-base'>Runs</h2>
          <p className='text-muted-foreground text-xs'>{runs.length} runs shown</p>
        </div>
        <Button
          variant='ghost'
          size='icon'
          onClick={refetch}
          className='h-8 w-8'
          tooltip={{ content: "Refresh" }}
        >
          <RefreshCw className='h-4 w-4' />
        </Button>
      </div>

      <div className='border-border border-b px-4 py-2'>
        <RunsFilterBar value={filters} onChange={setFilters} />
      </div>

      {canManage && selected.size > 0 && (
        <RunsBulkBar
          count={selected.size}
          busy={busy}
          retryableCount={retryableRuns.length}
          cancellableCount={cancellableRuns.length}
          onRetry={retrySelected}
          onCancel={cancelSelected}
          onClear={() => setSelected(new Set())}
        />
      )}

      {isPending ? (
        <LoadingState />
      ) : error ? (
        <ErrorState message='Failed to load runs' onRetry={refetch} />
      ) : (
        <div className='flex-1 overflow-y-auto'>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead className='w-8'>
                  <Checkbox
                    checked={allSelected}
                    disabled={!canManage || runs.length === 0}
                    onCheckedChange={toggleAll}
                    aria-label='Select all runs'
                  />
                </TableHead>
                <TableHead>Status</TableHead>
                <TableHead>Run</TableHead>
                <TableHead>Started</TableHead>
                <TableHead>Duration</TableHead>
                <TableHead>Trigger</TableHead>
                <TableHead className='w-8' />
              </TableRow>
            </TableHeader>
            <TableBody>
              {runs.length === 0 ? (
                <TableRow>
                  <TableCell colSpan={7}>
                    <div className='flex flex-col items-center gap-1.5 py-12 text-center text-muted-foreground'>
                      <ListChecks className='h-8 w-8 opacity-40' />
                      <p className='font-medium text-foreground text-sm'>
                        No runs match these filters
                      </p>
                      <p className='text-xs'>Widen the time range or clear the filters.</p>
                    </div>
                  </TableCell>
                </TableRow>
              ) : (
                runs.map((run) => (
                  <RunRow
                    key={run.runId}
                    run={run}
                    selectable={canManage}
                    selected={selected.has(run.runId)}
                    onToggle={toggle}
                  />
                ))
              )}
            </TableBody>
          </Table>
          {hasMore && (
            <div className='flex justify-center py-3'>
              <Button variant='outline' size='sm' onClick={() => setLimit((l) => l + PAGE)}>
                Load more
              </Button>
            </div>
          )}
        </div>
      )}
    </div>
  );
};

export default RunsPage;
