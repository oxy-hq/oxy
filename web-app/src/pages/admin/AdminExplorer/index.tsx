import { Search } from "lucide-react";
import { useEffect, useState } from "react";
import { Input } from "@/components/ui/shadcn/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue
} from "@/components/ui/shadcn/select";
import { Skeleton } from "@/components/ui/shadcn/skeleton";
import TablePagination from "@/components/ui/TablePagination";
import { useExplorerRuns, useExplorerThreads } from "@/hooks/api/adminExplorer";
import { cn } from "@/libs/utils/cn";
import { RunsTable } from "./components/RunsTable";
import { ThreadsTable } from "./components/ThreadsTable";
import {
  ALL,
  filterValue,
  PAGE_SIZE,
  RESOURCES,
  type Resource,
  RUN_SOURCE_TYPES,
  RUN_STATUSES,
  THREAD_SOURCE_TYPES,
  THREAD_STATUSES
} from "./constants";
import { useDebounced } from "./useDebounced";

/**
 * Cross-tenant data explorer — search the DB resources operators reach for
 * when debugging (threads + agentic runs today), each enriched with the
 * workspace / org / user it belongs to and a drill-in. The opposite of the
 * tenant lists: those answer "what exists?", this answers "find me THIS
 * broken thing and take me to it."
 */
export default function AdminExplorer() {
  const [resource, setResource] = useState<Resource>("threads");
  const [raw, setRaw] = useState("");
  const [status, setStatus] = useState(ALL);
  const [sourceType, setSourceType] = useState(ALL);
  const [page, setPage] = useState(1);
  const search = useDebounced(raw, 300);

  // Any change to the filters (or switching tabs) invalidates the current
  // page — jump back to page 1 rather than showing an out-of-range page.
  // biome-ignore lint/correctness/useExhaustiveDependencies: intentional reset on filter change
  useEffect(() => {
    setPage(1);
  }, [resource, search, status, sourceType]);

  const params = {
    search,
    status: filterValue(status),
    sourceType: filterValue(sourceType),
    page,
    pageSize: PAGE_SIZE
  };
  const threads = useExplorerThreads(params, { enabled: resource === "threads" });
  const runs = useExplorerRuns(params, { enabled: resource === "runs" });
  const active = resource === "threads" ? threads : runs;

  const statusOptions = resource === "threads" ? THREAD_STATUSES : RUN_STATUSES;
  const sourceOptions = resource === "threads" ? THREAD_SOURCE_TYPES : RUN_SOURCE_TYPES;
  const total = active.data?.total ?? 0;
  const totalPages = Math.max(1, Math.ceil(total / PAGE_SIZE));

  // If the match set shrank under us (e.g. a concurrent deletion while paging
  // deep), the current page can fall out of range and the server returns an
  // empty page. Clamp back into range — this re-fetches the last valid page
  // instead of stranding the user on an empty one.
  useEffect(() => {
    if (page > totalPages) setPage(totalPages);
  }, [page, totalPages]);

  return (
    <div className='mx-auto max-w-7xl space-y-5 p-6 lg:px-10 lg:py-8'>
      <header className='flex items-baseline gap-3'>
        <p className='font-medium text-[10px] text-muted-foreground uppercase tracking-[0.18em]'>
          Operations
        </p>
        <span className='text-muted-foreground/40'>/</span>
        <h1 className='font-semibold text-xl tracking-tight'>Explorer</h1>
      </header>

      <div className='flex flex-wrap items-center justify-between gap-3'>
        <div className='flex items-center gap-1'>
          {RESOURCES.map((r) => (
            <button
              key={r.id}
              type='button'
              onClick={() => {
                setResource(r.id);
                setStatus(ALL);
                setSourceType(ALL);
              }}
              className={cn(
                "rounded-md px-3 py-1.5 font-medium text-xs uppercase tracking-wide transition-colors",
                resource === r.id
                  ? "bg-foreground text-background"
                  : "text-muted-foreground hover:bg-muted hover:text-foreground"
              )}
            >
              {r.label}
            </button>
          ))}
        </div>
        <div className='flex flex-wrap items-center gap-2'>
          {!active.isPending ? (
            <span className='text-muted-foreground text-xs tabular-nums'>
              {total} {total === 1 ? "result" : "results"}
            </span>
          ) : null}
          <Select value={status} onValueChange={setStatus}>
            <SelectTrigger size='sm' className='w-36 text-xs' aria-label='Status filter'>
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {statusOptions.map((opt) => (
                <SelectItem key={opt.value} value={opt.value}>
                  {opt.label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          <Select value={sourceType} onValueChange={setSourceType}>
            <SelectTrigger size='sm' className='w-32 text-xs' aria-label='Source filter'>
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {sourceOptions.map((opt) => (
                <SelectItem key={opt.value} value={opt.value}>
                  {opt.label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          <div className='relative'>
            <Search className='absolute top-1/2 left-2 size-3.5 -translate-y-1/2 text-muted-foreground' />
            <Input
              value={raw}
              onChange={(e) => setRaw(e.target.value)}
              placeholder={
                resource === "threads"
                  ? "Search threads by title, content, id…"
                  : "Search runs by question, error, id…"
              }
              className='h-8 w-72 pl-7 font-mono text-[11px]'
              aria-label='Search'
            />
          </div>
        </div>
      </div>

      {active.isPending ? (
        <Skeleton className='h-64 w-full' />
      ) : active.isError ? (
        <div className='rounded-lg border border-destructive/40 bg-destructive/5 p-4 text-destructive text-xs'>
          Failed to load {resource}.
        </div>
      ) : (
        <>
          {resource === "threads" ? (
            <ThreadsTable rows={threads.data?.items ?? []} />
          ) : (
            <RunsTable rows={runs.data?.items ?? []} />
          )}
          <TablePagination
            currentPage={page}
            totalPages={totalPages}
            totalItems={total}
            pageSize={PAGE_SIZE}
            onPageChange={setPage}
            itemLabel={resource}
          />
        </>
      )}
    </div>
  );
}
