import { AlertTriangle } from "lucide-react";
import { cn } from "@/libs/shadcn/utils";
import type { AppStorageUsageRow } from "@/types/apps";
import { formatBytes, formatDelta, formatRelative } from "../utils";

type SortKey = "bytes" | "growth" | "untagged";

interface Props {
  rows: AppStorageUsageRow[];
  sort: SortKey;
  onSortChange: (sort: SortKey) => void;
  selectedAppId: string | null;
  onSelect: (appId: string) => void;
}

const COLUMNS: { key: SortKey; label: string; hint: string }[] = [
  { key: "bytes", label: "Size", hint: "Total measured bytes" },
  {
    key: "growth",
    label: "7d growth",
    hint: "Change over the trailing week — what predicts the next bill"
  },
  { key: "untagged", label: "Untagged", hint: "Bytes no retention rule covers" }
];

/**
 * Apps ranked by whichever number the operator is chasing. Growth is a
 * first-class sort, not an eyeball exercise: a 40 GB app that is flat costs
 * less attention than a 4 GB app doubling weekly.
 */
export function FleetTable({ rows, sort, onSortChange, selectedAppId, onSelect }: Props) {
  if (rows.length === 0) {
    return (
      <p className='px-2 py-1 text-muted-foreground text-xs'>
        No apps measured yet. The sweeper runs every 15 minutes, or use “Measure now”.
      </p>
    );
  }

  return (
    <table className='w-full text-xs' data-testid='admin-storage-fleet-table'>
      <thead className='sticky top-0 bg-background'>
        <tr className='border-b text-left text-muted-foreground text-xs'>
          <th className='px-2 py-1 font-medium'>App</th>
          {COLUMNS.map((c) => (
            <th key={c.key} className='px-2 py-1 text-right font-medium'>
              <button
                type='button'
                title={c.hint}
                onClick={() => onSortChange(c.key)}
                className={cn(
                  "hover:text-foreground",
                  sort === c.key && "text-foreground underline underline-offset-4"
                )}
              >
                {c.label}
              </button>
            </th>
          ))}
          <th className='px-2 py-1 text-right font-medium'>Measured</th>
        </tr>
      </thead>
      <tbody>
        {rows.map((row) => (
          <tr
            key={row.appId}
            onClick={() => onSelect(row.appId)}
            className={cn(
              "cursor-pointer border-b hover:bg-muted/50",
              selectedAppId === row.appId && "bg-muted"
            )}
            data-testid={`admin-storage-fleet-row-${row.appSlug}`}
          >
            <td className='px-2 py-1'>
              <div className='font-medium'>{row.appName || row.appSlug}</div>
              <div className='text-muted-foreground text-xs'>{row.orgName ?? row.orgId}</div>
            </td>
            <td className='px-2 py-1 text-right tabular-nums'>
              {formatBytes(row.bytes)}
              <div className='text-muted-foreground text-xs'>
                {row.objectCount.toLocaleString()} objects
              </div>
            </td>
            <td className='px-2 py-1 text-right tabular-nums'>{formatDelta(row.growthBytes7d)}</td>
            <td className='px-2 py-1 text-right tabular-nums'>
              {row.untaggedBytes > 0 ? (
                <span className='text-warning'>{formatBytes(row.untaggedBytes)}</span>
              ) : (
                <span className='text-muted-foreground'>—</span>
              )}
            </td>
            <td className='px-2 py-1 text-right text-muted-foreground text-xs'>
              {formatRelative(row.measuredAt)}
              {row.measureStatus !== "ok" && (
                // A partial walk means this row is a FLOOR. Saying nothing would
                // present an under-count as a total.
                <span
                  className='ml-1 inline-flex items-center gap-1 text-warning'
                  title={row.measureDetail ?? `measurement ${row.measureStatus}`}
                >
                  <AlertTriangle className='inline size-3' />
                  {row.measureStatus}
                </span>
              )}
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}
