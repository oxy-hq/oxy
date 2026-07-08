import { ChevronLeft, ChevronRight, Search, X } from "lucide-react";
import { useEffect, useState } from "react";
import { useWmFilterInstances, WM_FILTER_INSTANCES_PAGE_SIZE } from "@/hooks/api/useWorldModel";
import { cn } from "@/libs/shadcn/utils";
import type { WmInstance } from "@/types/worldModel";

interface SampleBrowserPopoverProps {
  /** The active/selected instance whose reachable rows we're browsing. */
  seedEntityId: string;
  seedKey: string;
  /** Target entity whose reachable rows to list. */
  entityId: string;
  entityLabel: string;
  /** Real reachable-row total (from the node card's filter count), for the header. */
  matched: number;
  position: { x: number; y: number };
  onPick: (instance: WmInstance) => void;
  onClose: () => void;
}

function useDebounce(value: string, ms = 300): string {
  const [debounced, setDebounced] = useState(value);
  useEffect(() => {
    if (!value) {
      setDebounced("");
      return;
    }
    const id = setTimeout(() => setDebounced(value), ms);
    return () => clearTimeout(id);
  }, [value, ms]);
  return debounced;
}

export function SampleBrowserPopover({
  seedEntityId,
  seedKey,
  entityId,
  entityLabel,
  matched,
  position,
  onPick,
  onClose
}: SampleBrowserPopoverProps) {
  const [search, setSearch] = useState("");
  const debouncedSearch = useDebounce(search);
  const [page, setPage] = useState(0);
  // A new search re-scopes the result set, so restart at the first page. Reset
  // in the input handlers (below) rather than an effect keyed on the search:
  // biome strips deps an effect body doesn't read, which would silently break
  // an effect whose only job is to react to the search changing.
  const changeSearch = (value: string) => {
    setSearch(value);
    setPage(0);
  };
  const { data, isLoading } = useWmFilterInstances(
    seedEntityId,
    seedKey,
    entityId,
    debouncedSearch,
    page
  );

  const rangeStart = page * WM_FILTER_INSTANCES_PAGE_SIZE;
  const canPrev = page > 0;
  const canNext = data?.has_more ?? false;

  const top = Math.max(8, position.y - 60);
  const left = position.x + 16;

  return (
    <>
      <div className='fixed inset-0 z-40' onClick={onClose} aria-hidden='true' />
      <div
        className='fixed z-50 flex w-64 flex-col border border-info bg-card shadow-[0_8px_32px_rgba(0,0,0,0.55)]'
        style={{ left, top, maxHeight: `min(320px, calc(100vh - ${top}px - 8px))` }}
        data-testid='wm-sample-browser'
      >
        {/* Header */}
        <div className='flex shrink-0 items-baseline justify-between gap-2 border-border border-b px-2.5 py-1.5 font-mono text-[10px] uppercase tracking-wider'>
          <span className='text-info'>Reachable {entityLabel}</span>
          <span className='text-muted-foreground'>{matched} total</span>
        </div>

        {/* Search */}
        <div className='flex shrink-0 items-center gap-1.5 border-border border-b bg-background/60 px-1.5'>
          <Search className='h-3 w-3 shrink-0 text-muted-foreground' />
          <input
            type='text'
            value={search}
            onChange={(e) => changeSearch(e.target.value)}
            placeholder={`Search ${entityLabel}s…`}
            className='h-7 flex-1 bg-transparent text-[11px] text-foreground placeholder:text-muted-foreground/60 focus:outline-none'
          />
          {search && (
            <span className='shrink-0 font-mono text-[9px] text-muted-foreground'>
              {data?.items.length ?? 0}
            </span>
          )}
          {search && (
            <button
              type='button'
              onClick={() => changeSearch("")}
              className='flex h-4 w-4 items-center justify-center text-muted-foreground hover:text-foreground'
              title='Clear'
            >
              <X className='h-2.5 w-2.5' />
            </button>
          )}
        </div>

        {/* List */}
        <ul className='flex min-h-0 flex-1 flex-col gap-0.5 overflow-y-auto p-1.5'>
          {isLoading ? (
            <li className='px-1 py-2 font-mono text-[10px] text-muted-foreground'>loading…</li>
          ) : !data?.items.length ? (
            <li className='px-1 py-2 font-mono text-[10px] text-muted-foreground'>
              {search ? `— no matches for "${search}"` : "No reachable rows"}
            </li>
          ) : (
            data.items.map((inst) => (
              <li key={inst.key}>
                <button
                  type='button'
                  className={cn(
                    "flex w-full min-w-0 items-center gap-1 border border-border bg-background/40 px-2 py-1 text-left text-[11px] hover:border-info"
                  )}
                  onClick={() => {
                    onPick(inst);
                    onClose();
                  }}
                  title={inst.key}
                >
                  <span className='shrink-0 text-info'>↓</span>
                  <span className='min-w-0 flex-1 truncate text-foreground'>{inst.display}</span>
                </button>
              </li>
            ))
          )}
        </ul>

        {/* Pager — driven by `has_more` (next) and the current page (prev). */}
        {(canPrev || canNext) && (
          <div className='flex shrink-0 items-center justify-between border-border border-t px-2 py-1 font-mono text-[9px] text-muted-foreground'>
            <button
              type='button'
              disabled={!canPrev}
              onClick={() => setPage((p) => Math.max(0, p - 1))}
              className='flex items-center gap-0.5 px-1 py-0.5 hover:text-foreground disabled:cursor-not-allowed disabled:opacity-30'
              title='Previous page'
            >
              <ChevronLeft className='h-2.5 w-2.5' />
              prev
            </button>
            <span className='tabular-nums'>
              {rangeStart + 1}–{rangeStart + (data?.items.length ?? 0)}
            </span>
            <button
              type='button'
              disabled={!canNext}
              onClick={() => setPage((p) => p + 1)}
              className='flex items-center gap-0.5 px-1 py-0.5 hover:text-foreground disabled:cursor-not-allowed disabled:opacity-30'
              title='Next page'
            >
              next
              <ChevronRight className='h-2.5 w-2.5' />
            </button>
          </div>
        )}
      </div>
    </>
  );
}
