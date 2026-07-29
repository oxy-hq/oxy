import { AlertTriangle, ChevronDown, ChevronUp, RefreshCw, X } from "lucide-react";
import { useMemo, useState } from "react";
import { Badge } from "@/components/ui/shadcn/badge";
import { Button } from "@/components/ui/shadcn/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue
} from "@/components/ui/shadcn/select";
import { Spinner } from "@/components/ui/shadcn/spinner";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow
} from "@/components/ui/shadcn/table";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/shadcn/tabs";
import {
  useMetricAnomalies,
  useMonitors,
  useScanMetricAnomalies,
  useUpdateAnomalyStatus
} from "@/hooks/api/useMetricAnomalies";
import type {
  AnomalyFilter,
  AnomalySeverity,
  AnomalyStatus,
  MetricAnomaly,
  ScanFailure
} from "@/types/metricAnomalies";
import ExplainDrawer from "./ExplainDrawer";
import MonitorsTab from "./MonitorsTab";

const STATUS_OPTIONS: { value: AnomalyStatus | "all"; label: string }[] = [
  { value: "new", label: "New" },
  { value: "acknowledged", label: "Acknowledged" },
  { value: "dismissed", label: "Dismissed" },
  { value: "all", label: "All" }
];

/**
 * Insights inbox — surfaces anomalies detected by `oxy-metric-monitoring`.
 * Contains two inner sub-tabs:
 *   - Inbox: anomaly list with status filter, scan trigger, explain drawer
 *   - Monitors: read-only list of `.monitor.yml` entries
 */
export default function AnomaliesInbox() {
  const [statusFilter, setStatusFilter] = useState<AnomalyStatus | "all">("new");
  const [asOf, setAsOf] = useState<string>("");
  const [selectedAnomaly, setSelectedAnomaly] = useState<MetricAnomaly | null>(null);
  const [failuresDismissed, setFailuresDismissed] = useState(false);

  const {
    data: anomalies,
    isLoading,
    error
  } = useMetricAnomalies(statusFilter === "all" ? undefined : statusFilter);
  const { data: monitors = [] } = useMonitors();
  const scanMutation = useScanMetricAnomalies();
  const triggerScan = () => {
    setFailuresDismissed(false);
    scanMutation.mutate(asOf || undefined);
  };
  const scanFailures = scanMutation.data?.failures ?? [];

  return (
    <div className='flex h-full min-h-0 flex-col overflow-hidden'>
      <Tabs defaultValue='inbox' className='flex h-full min-h-0 flex-col'>
        <TabsList className='w-full justify-start rounded-none border-border border-b bg-transparent px-4 py-0'>
          <TabsTrigger
            value='inbox'
            className='rounded-none border-transparent border-b-2 data-[state=active]:border-b-primary data-[state=active]:bg-transparent data-[state=active]:shadow-none'
          >
            Inbox
          </TabsTrigger>
          <TabsTrigger
            value='monitors'
            className='rounded-none border-transparent border-b-2 data-[state=active]:border-b-primary data-[state=active]:bg-transparent data-[state=active]:shadow-none'
          >
            Monitors
            {monitors.length > 0 && (
              <Badge variant='secondary' className='ml-1.5 rounded-full px-1.5 py-0 text-xs'>
                {monitors.length}
              </Badge>
            )}
          </TabsTrigger>
        </TabsList>

        <TabsContent value='inbox' className='mt-0 flex min-h-0 flex-1 flex-col'>
          <div className='flex items-center justify-between gap-2 border-border border-b px-4 py-2'>
            <div className='flex items-center gap-2'>
              <Select
                value={statusFilter}
                onValueChange={(v) => setStatusFilter(v as AnomalyStatus | "all")}
              >
                <SelectTrigger className='w-36'>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {STATUS_OPTIONS.map((opt) => (
                    <SelectItem key={opt.value} value={opt.value}>
                      {opt.label}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
              {anomalies && (
                <span className='text-muted-foreground text-sm'>
                  {anomalies.length} {anomalies.length === 1 ? "anomaly" : "anomalies"}
                </span>
              )}
            </div>
            <div className='flex items-center gap-2'>
              <label className='flex items-center gap-1 text-muted-foreground text-xs'>
                As of
                <input
                  type='date'
                  value={asOf}
                  onChange={(e) => setAsOf(e.target.value)}
                  className='h-8 rounded-md border border-input bg-background px-2 text-foreground text-sm'
                />
              </label>
              <Button
                size='sm'
                variant='outline'
                onClick={triggerScan}
                disabled={scanMutation.isPending}
              >
                {scanMutation.isPending ? (
                  <Spinner className='size-4' />
                ) : (
                  <RefreshCw className='size-4' />
                )}
                Scan now
              </Button>
            </div>
          </div>

          {!failuresDismissed && scanFailures.length > 0 && (
            <ScanFailuresBanner
              failures={scanFailures}
              onDismiss={() => setFailuresDismissed(true)}
            />
          )}

          <div className='flex-1 overflow-auto px-4 py-3'>
            {isLoading && (
              <div className='flex h-32 items-center justify-center'>
                <Spinner />
              </div>
            )}
            {error && (
              <div className='text-destructive text-sm'>
                {error instanceof Error ? error.message : "Failed to load anomalies."}
              </div>
            )}
            {!isLoading && !error && (!anomalies || anomalies.length === 0) && (
              <EmptyState onScan={triggerScan} scanning={scanMutation.isPending} />
            )}
            {anomalies && anomalies.length > 0 && (
              <AnomalyTable anomalies={anomalies} onExplain={setSelectedAnomaly} />
            )}
          </div>
        </TabsContent>

        <TabsContent value='monitors' className='mt-0 flex min-h-0 flex-1 flex-col'>
          <MonitorsTab />
        </TabsContent>
      </Tabs>

      <ExplainDrawer
        anomaly={selectedAnomaly}
        onOpenChange={(open) => {
          if (!open) setSelectedAnomaly(null);
        }}
      />
    </div>
  );
}

function EmptyState({ onScan, scanning }: { onScan: () => void; scanning: boolean }) {
  return (
    <div className='flex h-48 flex-col items-center justify-center gap-2 text-center'>
      <p className='text-muted-foreground text-sm'>No anomalies in this view.</p>
      <p className='text-muted-foreground text-xs'>
        Drop a <code className='t-code'>.monitor.yml</code> at the workspace root, then run a scan.
      </p>
      <Button size='sm' variant='outline' onClick={onScan} disabled={scanning}>
        {scanning ? <Spinner className='size-4' /> : <RefreshCw className='size-4' />}
        Scan now
      </Button>
    </div>
  );
}

/**
 * Persistent, dismissible banner listing the monitors that errored in the
 * last scan — the toast is transient and truncates, so this is where a user
 * sees *which* monitor failed and *why*. Cleared when the next scan starts.
 */
function ScanFailuresBanner({
  failures,
  onDismiss
}: {
  failures: ScanFailure[];
  onDismiss: () => void;
}) {
  return (
    <div className='mx-4 mt-3 rounded-md border border-orange-500/40 bg-orange-500/5 px-3 py-2'>
      <div className='flex items-start justify-between gap-2'>
        <div className='flex items-center gap-1.5 font-medium text-orange-600 text-sm dark:text-orange-400'>
          <AlertTriangle className='size-4 shrink-0' />
          {failures.length} monitor{failures.length === 1 ? "" : "s"} failed to scan
        </div>
        <Button
          size='icon'
          variant='ghost'
          className='size-6 text-muted-foreground'
          onClick={onDismiss}
          aria-label='Dismiss scan failures'
        >
          <X className='size-4' />
        </Button>
      </div>
      <ul className='mt-1.5 flex flex-col gap-1'>
        {failures.map((f) => {
          const segment = formatSegment(f);
          return (
            <li
              key={`${f.measure}:${f.time_dimension}:${f.granularity}:${f.dimension_key}`}
              className='text-muted-foreground text-xs'
            >
              <span className='font-medium text-foreground'>{f.label || f.measure}</span>
              {segment && <span className='ml-1 text-muted-foreground'>[{segment}]</span>}
              <span className='ml-1'>— {f.error}</span>
            </li>
          );
        })}
      </ul>
    </div>
  );
}

function AnomalyTable({
  anomalies,
  onExplain
}: {
  anomalies: MetricAnomaly[];
  onExplain: (a: MetricAnomaly) => void;
}) {
  return (
    <Table>
      <TableHeader>
        <TableRow>
          <TableHead className='w-24'>Severity</TableHead>
          <TableHead>Measure</TableHead>
          <TableHead>Period</TableHead>
          <TableHead className='text-right'>Observed</TableHead>
          <TableHead className='text-right'>Expected</TableHead>
          <TableHead className='text-right'>Δ%</TableHead>
          <TableHead className='w-72 text-right'>Actions</TableHead>
        </TableRow>
      </TableHeader>
      <TableBody>
        {groupIntoEvents(anomalies).map((event) => (
          <AnomalyRow key={event.peak.id} event={event} onExplain={onExplain} />
        ))}
      </TableBody>
    </Table>
  );
}

/** One anomaly event: the buckets it spans, and the worst of them. */
interface AnomalyEvent {
  /** Every bucket in the event, oldest first. */
  buckets: MetricAnomaly[];
  /** The bucket that departed furthest from expectation — what the row shows. */
  peak: MetricAnomaly;
}

/**
 * Collapse per-bucket rows into events.
 *
 * A sustained problem files one row per bucket, so a three-day labour surge
 * reads as three separate anomalies and "how many problems do I have" cannot be
 * answered by counting rows. Rows stay per-bucket on the server because
 * `explain` reasons about a single bucket, so the collapsing lives here.
 *
 * Rows with no `event_id` (detected before events existed) each stand alone
 * rather than being lumped together under a shared null key.
 */
function groupIntoEvents(anomalies: MetricAnomaly[]): AnomalyEvent[] {
  const byEvent = new Map<string, MetricAnomaly[]>();
  for (const a of anomalies) {
    const key = a.event_id ?? `ungrouped:${a.id}`;
    const existing = byEvent.get(key);
    if (existing) existing.push(a);
    else byEvent.set(key, [a]);
  }
  return Array.from(byEvent.values()).map((buckets) => {
    const ordered = [...buckets].sort((x, y) => x.period_start.localeCompare(y.period_start));
    // The peak, not the latest: the worst day is what an operator triages on,
    // and it is the bucket whose explain is worth reading.
    const peak = ordered.reduce((a, b) => (Math.abs(b.z_score) > Math.abs(a.z_score) ? b : a));
    return { buckets: ordered, peak };
  });
}

function AnomalyRow({
  event,
  onExplain
}: {
  event: AnomalyEvent;
  onExplain: (a: MetricAnomaly) => void;
}) {
  const update = useUpdateAnomalyStatus();
  const anomaly = event.peak;
  // Status actions apply to the whole event: acknowledging "the surge" and
  // leaving its other two days sitting in the inbox would defeat the grouping.
  const setStatus = (status: AnomalyStatus) => {
    for (const bucket of event.buckets) {
      update.mutate({ id: bucket.id, status });
    }
  };

  const deltaPct = useMemo(() => {
    if (Math.abs(anomaly.expected) < 1e-9) return null;
    return ((anomaly.observed - anomaly.expected) / Math.abs(anomaly.expected)) * 100;
  }, [anomaly.expected, anomaly.observed]);

  return (
    <TableRow data-status={anomaly.status}>
      <TableCell>
        <SeverityBadge severity={anomaly.severity} status={anomaly.status} />
      </TableCell>
      <TableCell>
        <div className='flex flex-col gap-1'>
          <span className='font-medium'>{anomaly.label || anomaly.measure}</span>
          <span className='text-muted-foreground text-xs'>{anomaly.measure}</span>
          <SegmentBadge anomaly={anomaly} />
        </div>
      </TableCell>
      <TableCell className='text-sm'>
        <div className='flex flex-col'>
          <span>{formatPeriod(anomaly)}</span>
          {event.buckets.length > 1 ? (
            <span className='text-muted-foreground text-xs'>
              {anomaly.granularity} · worst of {event.buckets.length} in this event
            </span>
          ) : (
            <span className='text-muted-foreground text-xs'>{anomaly.granularity}</span>
          )}
        </div>
      </TableCell>
      <TableCell className='t-code text-right'>{formatNumber(anomaly.observed)}</TableCell>
      <TableCell className='t-code text-right'>{formatNumber(anomaly.expected)}</TableCell>
      <TableCell className='t-code text-right'>
        {deltaPct === null ? "—" : <DeltaArrow value={deltaPct} />}
      </TableCell>
      <TableCell className='text-right'>
        <div className='flex justify-end gap-1'>
          <Button size='sm' variant='default' onClick={() => onExplain(anomaly)}>
            Explain
          </Button>
          {anomaly.status !== "acknowledged" && (
            <Button
              size='sm'
              variant='outline'
              disabled={update.isPending}
              onClick={() => setStatus("acknowledged")}
            >
              Ack
            </Button>
          )}
          {anomaly.status !== "dismissed" && (
            <Button
              size='sm'
              variant='ghost'
              disabled={update.isPending}
              onClick={() => setStatus("dismissed")}
            >
              Dismiss
            </Button>
          )}
        </div>
      </TableCell>
    </TableRow>
  );
}

/**
 * Chain-wide monitors have no segment; segment (`group_by` / filtered)
 * monitors render a chip so per-segment anomalies that share a measure/period
 * are visibly distinct instead of reading as duplicates.
 */
function SegmentBadge({ anomaly }: { anomaly: MetricAnomaly }) {
  const label = formatSegment(anomaly);
  if (!label) return null;
  return (
    <Badge variant='secondary' className='w-fit font-normal text-xs'>
      {label}
    </Badge>
  );
}

/** e.g. filters `[{member: "labor_daily.restaurant_id", values: ["loc-abc"]}]`
 *  → `"restaurant_id: loc-abc"`. Falls back to the raw `dimension_key`.
 *  Shared by anomaly rows and the scan-failures banner so the same segment
 *  reads identically on both surfaces. */
function formatSegment(segment: {
  filters: AnomalyFilter[] | null;
  dimension_key: string;
}): string | null {
  const { filters, dimension_key } = segment;
  if (filters && filters.length > 0) {
    return filters.map((f) => `${shortMember(f.member)}: ${f.values.join(", ")}`).join(" · ");
  }
  return dimension_key ? dimension_key : null;
}

/** Strip the `view.` prefix from a fully-qualified dimension id. */
function shortMember(member: string): string {
  const dot = member.lastIndexOf(".");
  return dot === -1 ? member : member.slice(dot + 1);
}

function SeverityBadge({ severity, status }: { severity: AnomalySeverity; status: AnomalyStatus }) {
  const tone = status === "dismissed" ? "muted" : severityTone(severity);
  return (
    <Badge variant={tone === "destructive" ? "destructive" : "outline"} className={toneClass(tone)}>
      {severity}
    </Badge>
  );
}

type Tone = "destructive" | "warning" | "info" | "muted";

function severityTone(severity: AnomalySeverity): Tone {
  switch (severity) {
    case "high":
      return "destructive";
    case "medium":
      return "warning";
    case "low":
      return "info";
  }
}

function toneClass(tone: Tone): string {
  switch (tone) {
    case "destructive":
      return "";
    case "warning":
      return "border-orange-500/40 text-orange-600 dark:text-orange-400";
    case "info":
      return "border-sky-500/40 text-sky-600 dark:text-sky-400";
    case "muted":
      return "border-muted text-muted-foreground";
  }
}

function DeltaArrow({ value }: { value: number }) {
  const positive = value >= 0;
  const Icon = positive ? ChevronUp : ChevronDown;
  const colorClass = positive ? "text-emerald-600 dark:text-emerald-400" : "text-destructive";
  return (
    <span className={`inline-flex items-center justify-end gap-0.5 ${colorClass}`}>
      <Icon className='size-3' />
      {Math.abs(value).toFixed(1)}%
    </span>
  );
}

function formatNumber(n: number): string {
  if (Math.abs(n) >= 1_000_000) return `${(n / 1_000_000).toFixed(2)}M`;
  if (Math.abs(n) >= 1_000) return `${(n / 1_000).toFixed(2)}k`;
  return n.toFixed(2);
}

function formatPeriod(a: MetricAnomaly): string {
  const start = new Date(a.period_start);
  if (a.granularity === "day") {
    return start.toISOString().slice(0, 10);
  }
  if (a.granularity === "week") {
    return `Week of ${start.toISOString().slice(0, 10)}`;
  }
  return start.toLocaleDateString("en-US", { year: "numeric", month: "short" });
}
