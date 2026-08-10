import { useMemo } from "react";
import { Badge } from "@/components/ui/shadcn/badge";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow
} from "@/components/ui/shadcn/table";
import {
  useMetricAnomalies,
  useMonitorCoverage,
  useMonitors
} from "@/hooks/api/useMetricAnomalies";
import type { AnomalyFilter, MonitorCoverage, MonitorEntry } from "@/types/metricAnomalies";

/** Mirror of `MonitorFilter::key_for` (crates/metric-monitoring/src/config.rs):
 *  `member=v1,v2` pairs with values sorted, pairs sorted, joined by `;`. This
 *  is the string the scanner stores as a coverage row's `dimension_key`. */
function filterKey(filters: AnomalyFilter[] | null | undefined): string {
  if (!filters || filters.length === 0) return "";
  return filters
    .map((f) => `${f.member}=${[...f.values].sort().join(",")}`)
    .sort()
    .join(";");
}

/** The coverage rows belonging to one monitor entry, within rows already
 *  narrowed to its (measure, time_dimension, granularity) triple.
 *
 *  An entry without `group_by` has exactly one segment, so it matches its own
 *  filter key exactly — that is what keeps two entries differing only by
 *  `filters` (region=US vs region=EU) from rendering each other's segments. A
 *  `group_by` entry fans out at scan time, so its segments carry its filters
 *  *plus* the discovered value and are matched by containment instead. */
function coverageFor(entry: MonitorEntry, rows: MonitorCoverage[]): MonitorCoverage[] {
  const own = filterKey(entry.filters);
  if (!entry.group_by) return rows.filter((c) => c.dimension_key === own);
  if (own === "") return rows;
  const required = own.split(";");
  return rows.filter((c) => {
    const pairs = new Set(c.dimension_key ? c.dimension_key.split(";") : []);
    return required.every((p) => pairs.has(p));
  });
}

function sensitivityVariant(s: MonitorEntry["sensitivity"]) {
  if (s === "high") return "destructive" as const;
  if (s === "low") return "secondary" as const;
  return "outline" as const;
}

function bucketNoun(granularity: string, n: number): string {
  const base = granularity === "week" ? "week" : granularity === "month" ? "month" : "day";
  return n === 1 ? base : `${base}s`;
}

/** What to show in the Coverage column for one monitor's segments.
 *
 *  `null` means "nothing to say" — either the monitor is being scored normally
 *  or it has never been scanned. Only the warming-up case earns a badge, since
 *  that is the state an empty inbox would otherwise hide. */
function warmingSummary(rows: MonitorCoverage[]): { label: string; detail: string } | null {
  const warming = rows.filter((c) => c.measured_buckets < c.required_buckets);
  if (warming.length === 0) return null;

  // A monitor without group_by has exactly one segment, so name the real
  // numbers rather than an unhelpful "1 of 1 segments".
  if (rows.length === 1) {
    const only = warming[0];
    return {
      label: "Warming up",
      detail: `${only.measured_buckets} of ${only.required_buckets} ${bucketNoun(
        only.granularity,
        only.required_buckets
      )}`
    };
  }

  // Fanned out by group_by. The count alone hides how long the wait is, so
  // report the segment furthest from clearing the floor alongside it.
  const furthest = warming.reduce((a, b) =>
    a.required_buckets - a.measured_buckets >= b.required_buckets - b.measured_buckets ? a : b
  );
  return {
    label: warming.length === rows.length ? "Warming up" : "Partly warming up",
    detail: `${warming.length} of ${rows.length} segments · furthest ${furthest.measured_buckets} of ${furthest.required_buckets}`
  };
}

function relativeTime(isoDate: string): string {
  const days = Math.floor((Date.now() - new Date(isoDate).getTime()) / 86_400_000);
  if (days === 0) return "today";
  if (days === 1) return "1 day ago";
  return `${days} days ago`;
}

export default function MonitorsTab() {
  const { data: monitors = [], isLoading, error } = useMonitors();
  const { data: coverage = [] } = useMonitorCoverage();
  // Fetch anomalies latest-first (no status filter), then take the max
  // `period_start` (the anomaly's bucket date — what the column shows) per
  // measure across all statuses. `order="recent"` is required: the Inbox's
  // default severity ranking would let a measure whose recent anomalies are all
  // `low` fall off the page and show a stale "last anomaly".
  const { data: anomalies = [] } = useMetricAnomalies(undefined, "recent");

  const lastAnomalyByMeasure = useMemo(() => {
    const map = new Map<string, string>();
    for (const a of anomalies) {
      const existing = map.get(a.measure);
      if (!existing || a.period_start > existing) {
        map.set(a.measure, a.period_start);
      }
    }
    return map;
  }, [anomalies]);

  // Coverage is per segment; the table is per monitor entry. Group on the same
  // (measure, time_dimension, granularity) triple the scanner keys its rows by
  // — granularity included because a daily and a weekly monitor over the same
  // measure have different floors (56 buckets vs 26) and must not be merged.
  // The triple alone does not identify an entry, so `coverageFor` narrows
  // further by filters at lookup time.
  const coverageByMonitor = useMemo(() => {
    const map = new Map<string, MonitorCoverage[]>();
    for (const c of coverage) {
      const key = `${c.measure}:${c.time_dimension}:${c.granularity}`;
      const existing = map.get(key);
      if (existing) existing.push(c);
      else map.set(key, [c]);
    }
    return map;
  }, [coverage]);

  if (isLoading) {
    return (
      <div className='flex h-32 items-center justify-center text-muted-foreground text-sm'>
        Loading monitors…
      </div>
    );
  }

  if (error) {
    return (
      <div className='px-4 py-3 text-destructive text-sm'>
        {error instanceof Error ? error.message : "Failed to load monitor config."}
      </div>
    );
  }

  if (monitors.length === 0) {
    return (
      <div className='flex h-48 flex-col items-center justify-center gap-2 text-center text-muted-foreground text-sm'>
        <p>No monitors configured.</p>
        <p>
          Drop a{" "}
          <code className='rounded bg-muted px-1 py-0.5 font-mono text-xs'>.monitor.yml</code> at
          the workspace root to start.
        </p>
      </div>
    );
  }

  return (
    <div className='flex-1 overflow-auto px-4 py-3'>
      <Table>
        <TableHeader>
          <TableRow>
            <TableHead>Metric</TableHead>
            <TableHead className='w-24'>Granularity</TableHead>
            <TableHead className='w-28'>Sensitivity</TableHead>
            <TableHead className='w-48'>Coverage</TableHead>
            <TableHead className='w-32 text-right'>Last anomaly</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {monitors.map((m) => {
            const lastAt = lastAnomalyByMeasure.get(m.measure);
            const warming = warmingSummary(
              coverageFor(
                m,
                coverageByMonitor.get(`${m.measure}:${m.time_dimension}:${m.granularity}`) ?? []
              )
            );
            return (
              // Two entries can share a measure and time-dimension and differ
              // only by granularity or filters, so the key carries both. That is
              // the full identity of an entry — a .monitor.yml that repeats one
              // verbatim declares the same monitor twice.
              <TableRow
                key={`${m.measure}:${m.time_dimension}:${m.granularity}:${filterKey(m.filters)}`}
              >
                <TableCell>
                  <p className='font-medium'>{m.label ?? m.measure}</p>
                  <p className='text-muted-foreground text-xs'>
                    {m.measure} · {m.time_dimension}
                  </p>
                </TableCell>
                <TableCell className='text-muted-foreground'>{m.granularity}</TableCell>
                <TableCell>
                  <Badge variant={sensitivityVariant(m.sensitivity)}>{m.sensitivity}</Badge>
                </TableCell>
                <TableCell>
                  {warming ? (
                    <>
                      <Badge variant='secondary'>{warming.label}</Badge>
                      <p className='mt-0.5 text-muted-foreground text-xs'>{warming.detail}</p>
                    </>
                  ) : (
                    <span className='text-muted-foreground text-sm'>—</span>
                  )}
                </TableCell>
                <TableCell className='text-right text-sm'>
                  {lastAt ? (
                    <span className='text-muted-foreground'>{relativeTime(lastAt)}</span>
                  ) : (
                    <span className='text-muted-foreground'>—</span>
                  )}
                </TableCell>
              </TableRow>
            );
          })}
        </TableBody>
      </Table>
    </div>
  );
}
