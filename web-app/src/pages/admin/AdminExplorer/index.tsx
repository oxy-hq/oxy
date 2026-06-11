import { Search } from "lucide-react";
import { useState } from "react";
import { Skeleton } from "@/components/ui/shadcn/skeleton";
import { useExplorerRuns, useExplorerThreads } from "@/hooks/api/adminExplorer";
import { cn } from "@/libs/utils/cn";
import { RunsTable } from "./components/RunsTable";
import { ThreadsTable } from "./components/ThreadsTable";
import { useDebounced } from "./useDebounced";

type Resource = "threads" | "runs";
const RESOURCES: { id: Resource; label: string }[] = [
  { id: "threads", label: "Threads" },
  { id: "runs", label: "Runs" }
];

const RUN_STATUSES = ["", "failed", "running", "done", "cancelled"] as const;

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
  const [status, setStatus] = useState("");
  const search = useDebounced(raw, 300);

  const threads = useExplorerThreads(search, { enabled: resource === "threads" });
  const runs = useExplorerRuns(search, status, { enabled: resource === "runs" });
  const active = resource === "threads" ? threads : runs;

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
              onClick={() => setResource(r.id)}
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
        <div className='flex items-center gap-2'>
          {resource === "runs" ? (
            <select
              value={status}
              onChange={(e) => setStatus(e.target.value)}
              className='h-8 rounded-md border border-border/60 bg-card px-2 text-xs outline-none focus:border-border'
              aria-label='Run status filter'
            >
              {RUN_STATUSES.map((s) => (
                <option key={s || "all"} value={s}>
                  {s === "" ? "All statuses" : s}
                </option>
              ))}
            </select>
          ) : null}
          <div className='relative'>
            <Search className='absolute top-1/2 left-2 size-3.5 -translate-y-1/2 text-muted-foreground' />
            <input
              value={raw}
              onChange={(e) => setRaw(e.target.value)}
              placeholder={
                resource === "threads"
                  ? "Search threads by title, content, id…"
                  : "Search runs by question, error, id…"
              }
              className='h-8 w-80 rounded-md border border-border/60 bg-card pr-2 pl-7 font-mono text-[11px] outline-none placeholder:text-muted-foreground/60 focus:border-border focus:ring-1 focus:ring-ring'
              aria-label='Search'
            />
          </div>
        </div>
      </div>

      {active.isPending ? (
        <Skeleton className='h-64 w-full' />
      ) : active.isError ? (
        <div className='rounded-lg border border-destructive/40 bg-destructive/5 p-4 text-destructive text-sm'>
          Failed to load {resource}.
        </div>
      ) : resource === "threads" ? (
        <ThreadsTable rows={threads.data ?? []} />
      ) : (
        <RunsTable rows={runs.data ?? []} />
      )}
    </div>
  );
}
