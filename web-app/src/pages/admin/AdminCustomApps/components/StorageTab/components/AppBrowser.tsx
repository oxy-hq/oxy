import { Loader2, Trash2 } from "lucide-react";
import { useMemo, useState } from "react";
import { Button } from "@/components/ui/shadcn/button";
import {
  useAppStorageObjects,
  useDeleteStorageObjects
} from "@/hooks/api/customApps/useAppStorage";
import type { AppStorageUsageRow } from "@/types/apps";
import { daysUntilExpiry, formatBytes, formatRelative } from "../utils";
import { PrefixBreakdown } from "./PrefixBreakdown";

interface Props {
  app: AppStorageUsageRow;
}

/**
 * One app's objects, read live from S3.
 *
 * Deliberately not backed by the rollup: the rollup holds no per-object rows,
 * and an operator here is investigating *now* — a listing up to a sweep old
 * would show files they just deleted.
 */
export function AppBrowser({ app }: Props) {
  const [prefix, setPrefix] = useState("");
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const { data, isLoading, hasNextPage, isFetchingNextPage, fetchNextPage } = useAppStorageObjects(
    app.appId,
    prefix
  );
  const remove = useDeleteStorageObjects(app.appId);

  const objects = useMemo(() => data?.pages.flatMap((p) => p.objects) ?? [], [data]);
  const retentionRules = data?.pages[0]?.retentionRules ?? [];

  const toggle = (key: string) =>
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });

  const onDelete = async () => {
    const keys = [...selected];
    if (keys.length === 0) return;
    if (!window.confirm(`Permanently delete ${keys.length} object(s) from ${app.appName}?`)) {
      return;
    }
    await remove.mutateAsync(keys);
    setSelected(new Set());
  };

  return (
    <div className='flex h-full flex-col' data-testid='admin-storage-app-browser'>
      <div className='border-b p-2'>
        <div className='flex items-baseline justify-between'>
          <h3 className='font-semibold text-sm'>{app.appName || app.appSlug}</h3>
          <span className='text-muted-foreground text-xs tabular-nums'>
            {formatBytes(app.bytes)} · {app.objectCount.toLocaleString()} objects
          </span>
        </div>
        <PrefixBreakdown breakdown={app.prefixBreakdown} untaggedBytes={app.untaggedBytes} />
        {retentionRules.length === 0 && (
          // The actionable gap: no rules means nothing in this silo will ever be
          // reclaimed, which is exactly the state that produces a surprise bill.
          <p className='mt-1 text-warning text-xs'>
            No <code>storage.retention</code> rules in this app’s oxy-app.json — nothing here
            expires.
          </p>
        )}
      </div>

      <div className='flex items-center gap-2 border-b p-2'>
        <input
          value={prefix}
          onChange={(e) => setPrefix(e.target.value)}
          placeholder='Filter by prefix, e.g. generated/'
          className='flex-1 rounded border bg-transparent px-2 py-1 text-xs'
          data-testid='admin-storage-prefix-filter'
        />
        <Button
          variant='destructive'
          size='sm'
          disabled={selected.size === 0 || remove.isPending}
          onClick={onDelete}
          data-testid='admin-storage-delete-selected'
        >
          <Trash2 className='mr-1 size-3' />
          Delete {selected.size > 0 ? selected.size : ""}
        </Button>
      </div>

      <div className='flex-1 overflow-auto'>
        {isLoading ? (
          <div className='flex items-center gap-2 p-2 text-muted-foreground text-xs'>
            <Loader2 className='size-3 animate-spin' /> Listing objects…
          </div>
        ) : objects.length === 0 ? (
          <p className='p-2 text-muted-foreground text-xs'>No objects under this prefix.</p>
        ) : (
          <table className='w-full text-xs'>
            <tbody>
              {objects.map((o) => {
                const days = daysUntilExpiry(o.expireAfter, o.lastModified);
                return (
                  <tr key={o.key} className='border-b hover:bg-muted/50'>
                    <td className='w-8 px-2 py-1'>
                      <input
                        type='checkbox'
                        checked={selected.has(o.key)}
                        onChange={() => toggle(o.key)}
                        aria-label={`Select ${o.path}`}
                      />
                    </td>
                    <td className='px-2 py-1 font-mono text-xs'>{o.path}</td>
                    <td className='px-2 py-1 text-right tabular-nums'>{formatBytes(o.size)}</td>
                    <td className='px-2 py-1 text-right text-muted-foreground text-xs'>
                      {formatRelative(o.lastModified)}
                    </td>
                    <td className='px-2 py-1 text-right text-xs'>
                      {days === null ? (
                        <span className='text-muted-foreground'>keeps</span>
                      ) : (
                        // Approximate on purpose: S3 evaluates tag-filtered
                        // rules daily, not on the hour.
                        <span title={`Retention class ${o.expireAfter}`}>~{days}d</span>
                      )}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        )}
        {hasNextPage && (
          <div className='p-2'>
            <Button
              variant='outline'
              size='sm'
              onClick={() => fetchNextPage()}
              disabled={isFetchingNextPage}
            >
              {isFetchingNextPage ? "Loading…" : "Load more"}
            </Button>
          </div>
        )}
      </div>
    </div>
  );
}
