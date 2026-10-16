import { Switch } from "@/components/ui/shadcn/switch";
import { cn } from "@/libs/shadcn/utils";
import type { NotificationChannel, NotificationReach } from "@/types/notifications";
import { CHANNELS, type PrefRow } from "../utils";

/** Indent per depth, on the spacing scale: the source, a category, a kind below it, … */
const INDENT = ["pl-4", "pl-8", "pl-12", "pl-16", "pl-20"];

interface PrefRowViewProps {
  row: PrefRow;
  /** The source's name, for each switch's accessible name. */
  source: string;
  /** Names the row's test ids: an app's slug, or `oxygen`. */
  testKey: string;
  reach: NotificationReach;
  onChange: (channel: NotificationChannel, muted: boolean) => void;
}

/** "Critical — always delivered", or "Muted by <row above>", or nothing. */
function rowNote(row: PrefRow): string | null {
  if (row.critical) return "Critical — always delivered";
  const by = [...new Set(Object.values(row.mutedBy))];
  return by.length > 0 ? `Muted by ${by.join(", ")}` : null;
}

/**
 * One row of a source's settings: a Push / Email / Slack switch each (on = not
 * muted), for the channels something under the row is sent on.
 *
 * A critical row is locked on. A channel a row above already mutes shows off
 * and locked, saying which row to change instead. Email cannot move for an
 * account with no address.
 */
export function PrefRowView({ row, source, testKey, reach, onChange }: PrefRowViewProps) {
  const note = rowNote(row);
  const name = row.depth === 0 ? source : `${source}, ${row.label}`;
  const testId = `notifications-pref-${testKey}-${row.key}`;

  return (
    <div
      className={cn(
        "flex items-center gap-2 border-b py-2.5 pr-4 last:border-b-0",
        INDENT[Math.min(row.depth, INDENT.length - 1)],
        row.depth === 0 && "bg-muted/30"
      )}
      data-testid={`${testId}-row`}
    >
      <span className='flex min-w-0 flex-1 flex-col'>
        <span className={cn("truncate text-sm", row.depth <= 1 && "font-medium")}>{row.label}</span>
        {row.description && (
          <span className='truncate text-muted-foreground text-xs'>{row.description}</span>
        )}
        {note && (
          <span className='text-muted-foreground text-xs' data-testid={`${testId}-note`}>
            {note}
          </span>
        )}
      </span>
      {CHANNELS.map((c) => {
        const by = row.critical ? undefined : row.mutedBy[c.value];
        return (
          <span key={c.value} className='flex w-12 justify-center sm:w-14'>
            {row.channels.includes(c.value) ? (
              <Switch
                checked={row.critical || (!by && !row.muted.includes(c.value))}
                disabled={row.critical || !!by || (c.value === "email" && !reach.email)}
                aria-label={by ? `${name}: ${c.label}, muted by ${by}` : `${name}: ${c.label}`}
                data-testid={`${testId}-${c.value}`}
                onCheckedChange={(checked) => onChange(c.value, !checked)}
              />
            ) : (
              <span
                className='text-muted-foreground text-xs'
                data-testid={`${testId}-${c.value}-none`}
              >
                <span aria-hidden>—</span>
                <span className='sr-only'>{`${name}: not sent by ${c.label}`}</span>
              </span>
            )}
          </span>
        );
      })}
    </div>
  );
}
