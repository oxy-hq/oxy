import { Search, X } from "lucide-react";
import { useEffect, useState } from "react";
import { useWmInstances } from "@/hooks/api/useWorldModel";
import { cn } from "@/libs/shadcn/utils";
import type { WmInstance } from "@/types/worldModel";

interface InstancePickerPopoverProps {
  entityId: string;
  entityLabel: string;
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

export function InstancePickerPopover({
  entityId,
  entityLabel,
  position,
  onPick,
  onClose
}: InstancePickerPopoverProps) {
  const [search, setSearch] = useState("");
  const debouncedSearch = useDebounce(search);
  const { data, isLoading } = useWmInstances(entityId, debouncedSearch);

  const top = Math.max(8, position.y - 60);
  const left = position.x + 16;

  return (
    <>
      <div className='fixed inset-0 z-40' onClick={onClose} aria-hidden='true' />
      <div
        className='fixed z-50 flex w-64 flex-col border border-info bg-card shadow-[0_8px_32px_rgba(0,0,0,0.55)]'
        style={{ left, top, maxHeight: `min(320px, calc(100vh - ${top}px - 8px))` }}
        data-testid='wm-instance-picker'
      >
        {/* Header */}
        <div className='flex shrink-0 items-baseline justify-between gap-2 border-border border-b px-2.5 py-1.5 font-mono text-[10px] uppercase tracking-wider'>
          <span className='text-info'>Pick {entityLabel}</span>
          {data && (
            <span className='text-muted-foreground'>
              {data.total}
              {data.has_more ? "+" : ""} total
            </span>
          )}
        </div>

        {/* Search */}
        <div className='flex shrink-0 items-center gap-1.5 border-border border-b bg-background/60 px-1.5'>
          <Search className='h-3 w-3 shrink-0 text-muted-foreground' />
          <input
            type='text'
            value={search}
            onChange={(e) => setSearch(e.target.value)}
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
              onClick={() => setSearch("")}
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
              {search ? `— no matches for "${search}"` : "No instances"}
            </li>
          ) : (
            data.items.map((inst) => (
              <li key={inst.key}>
                <button
                  type='button'
                  className={cn(
                    "flex w-full min-w-0 flex-col items-start gap-0.5 border border-border bg-background/40 px-2 py-1 text-left text-[11px] hover:border-info"
                  )}
                  onClick={() => {
                    onPick(inst);
                    onClose();
                  }}
                  title={inst.key}
                >
                  <span className='w-full truncate text-foreground'>{inst.display}</span>
                  {inst.display !== inst.key && (
                    <span className='w-full truncate font-mono text-[9px] text-muted-foreground'>
                      {inst.key}
                    </span>
                  )}
                </button>
              </li>
            ))
          )}
        </ul>
      </div>
    </>
  );
}
