import { useMemo } from "react";
import EChart from "@/components/Echarts/EChart";
import useTheme from "@/stores/useTheme";
import type { StoragePrefixUsage } from "@/types/apps";
import { type PrefixSlice, prefixCompositionOption } from "../chartSpec";
import { formatBytes } from "../utils";

interface Props {
  breakdown: Record<string, StoragePrefixUsage> | null;
  untaggedBytes: number;
}

/**
 * Size split by top-level prefix, with each one's retention class.
 *
 * This is the part that changes behaviour. A flat list of 18,000 objects tells
 * an operator nothing; "generated/ is 1.2 GiB and expires, uploads/ is 2.9 GiB
 * and does not" points straight at the manifest line to edit.
 *
 * A composition bar rather than a pie: the comparison that matters is between
 * segment lengths, which people read accurately, not between angles, which they
 * do not. The bar is paired with the rows beneath it — identity comes from the
 * labels, never from hue alone.
 */
export function PrefixBreakdown({ breakdown, untaggedBytes }: Props) {
  const entries = useMemo(
    () => Object.entries(breakdown ?? {}).sort((a, b) => b[1].bytes - a[1].bytes),
    [breakdown]
  );
  const slices: PrefixSlice[] = useMemo(
    () =>
      entries.map(([prefix, usage]) => ({
        prefix,
        bytes: usage.bytes,
        expireAfter: usage.expireAfter
      })),
    [entries]
  );
  // Same reason as UsageChart: the segment fills and their surface-colored
  // gaps are resolved at build time and would keep the old theme's values.
  const theme = useTheme((s) => s.theme);
  const option = useMemo(() => prefixCompositionOption(slices, theme), [slices, theme]);

  if (entries.length === 0) return null;

  return (
    <div className='mt-2' data-testid='admin-storage-prefix-breakdown'>
      <EChart option={option} height={22} />
      <div className='mt-2 space-y-1'>
        {entries.map(([prefix, usage]) => (
          <div key={prefix} className='flex items-baseline justify-between text-xs'>
            <span className='font-mono'>{prefix}</span>
            <span className='flex items-baseline gap-2'>
              <span className='text-muted-foreground tabular-nums'>
                {formatBytes(usage.bytes)} · {usage.objects.toLocaleString()}
              </span>
              {usage.expireAfter ? (
                <span className='text-muted-foreground'>expires {usage.expireAfter}</span>
              ) : (
                <span className='text-warning'>keeps forever</span>
              )}
            </span>
          </div>
        ))}
      </div>
      {untaggedBytes > 0 && (
        <div className='mt-1 flex items-baseline justify-between border-t pt-1 text-xs'>
          <span className='text-warning'>no retention rule</span>
          <span className='text-warning tabular-nums'>{formatBytes(untaggedBytes)}</span>
        </div>
      )}
    </div>
  );
}
