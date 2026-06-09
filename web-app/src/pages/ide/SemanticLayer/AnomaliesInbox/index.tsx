import { ChevronDown, ChevronUp, RefreshCw } from "lucide-react";
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
import type { AnomalySeverity, AnomalyStatus, MetricAnomaly } from "@/types/metricAnomalies";
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

  const {
    data: anomalies,
    isLoading,
    error
  } = useMetricAnomalies(statusFilter === "all" ? undefined : statusFilter);
  const { data: monitors = [] } = useMonitors();
  const scanMutation = useScanMetricAnomalies();
  const triggerScan = () => scanMutation.mutate(asOf || undefined);

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
        {anomalies.map((a) => (
          <AnomalyRow key={a.id} anomaly={a} onExplain={onExplain} />
        ))}
      </TableBody>
    </Table>
  );
}

function AnomalyRow({
  anomaly,
  onExplain
}: {
  anomaly: MetricAnomaly;
  onExplain: (a: MetricAnomaly) => void;
}) {
  const update = useUpdateAnomalyStatus();

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
        <div className='flex flex-col'>
          <span className='font-medium'>{anomaly.label || anomaly.measure}</span>
          <span className='text-muted-foreground text-xs'>{anomaly.measure}</span>
        </div>
      </TableCell>
      <TableCell className='text-sm'>
        <div className='flex flex-col'>
          <span>{formatPeriod(anomaly)}</span>
          <span className='text-muted-foreground text-xs'>{anomaly.granularity}</span>
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
              onClick={() => update.mutate({ id: anomaly.id, status: "acknowledged" })}
            >
              Ack
            </Button>
          )}
          {anomaly.status !== "dismissed" && (
            <Button
              size='sm'
              variant='ghost'
              disabled={update.isPending}
              onClick={() => update.mutate({ id: anomaly.id, status: "dismissed" })}
            >
              Dismiss
            </Button>
          )}
        </div>
      </TableCell>
    </TableRow>
  );
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
