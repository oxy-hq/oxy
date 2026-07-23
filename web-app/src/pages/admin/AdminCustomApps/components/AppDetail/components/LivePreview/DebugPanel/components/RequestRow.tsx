import { cn } from "@/libs/shadcn/utils";
import type { OxyRequestEntry } from "../../useOxyRequestLog";
import { fmtTime, statusLabel, statusTone } from "../format";

/**
 * One request line in the network list. Clickable to open the detail pane.
 * `compact` drops the time + latency columns so the list can shrink to ~40%
 * width once a request is selected and the detail pane claims the rest.
 */
export const RequestRow = ({
  entry,
  selected,
  compact,
  onSelect
}: {
  entry: OxyRequestEntry;
  selected: boolean;
  compact: boolean;
  onSelect: () => void;
}) => (
  <button
    type='button'
    onClick={onSelect}
    className={cn(
      "grid w-full items-center gap-3 px-3 py-1 text-left font-mono text-xs",
      compact ? "grid-cols-[3rem_1fr_auto]" : "grid-cols-[3.5rem_3rem_1fr_auto_auto]",
      selected ? "bg-primary/10 text-foreground" : "odd:bg-muted/20 hover:bg-muted/50"
    )}
  >
    {!compact && <span className='text-muted-foreground/50'>{fmtTime(entry.at)}</span>}
    <span className='font-medium text-muted-foreground'>{entry.method}</span>
    <span className='truncate' title={entry.url}>
      {entry.path}
    </span>
    <span className={cn("tabular-nums", statusTone(entry))}>{statusLabel(entry)}</span>
    {!compact && (
      <span className='w-12 text-right text-muted-foreground/70 tabular-nums'>
        {entry.ms == null ? "" : `${entry.ms}ms`}
      </span>
    )}
  </button>
);
