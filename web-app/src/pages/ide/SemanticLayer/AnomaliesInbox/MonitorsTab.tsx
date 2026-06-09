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
import { useMetricAnomalies, useMonitors } from "@/hooks/api/useMetricAnomalies";
import type { MonitorEntry } from "@/types/metricAnomalies";

function sensitivityVariant(s: MonitorEntry["sensitivity"]) {
  if (s === "high") return "destructive" as const;
  if (s === "low") return "secondary" as const;
  return "outline" as const;
}

function relativeTime(isoDate: string): string {
  const days = Math.floor((Date.now() - new Date(isoDate).getTime()) / 86_400_000);
  if (days === 0) return "today";
  if (days === 1) return "1 day ago";
  return `${days} days ago`;
}

export default function MonitorsTab() {
  const { data: monitors = [], isLoading, error } = useMonitors();
  // Fetch all anomalies (no status filter) to find the most recent
  // detected_at per measure across all statuses.
  const { data: anomalies = [] } = useMetricAnomalies(undefined);

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
            <TableHead className='w-32 text-right'>Last anomaly</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {monitors.map((m) => {
            const lastAt = lastAnomalyByMeasure.get(m.measure);
            return (
              <TableRow key={`${m.measure}:${m.time_dimension}`}>
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
