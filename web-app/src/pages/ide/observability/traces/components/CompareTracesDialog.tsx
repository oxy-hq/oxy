import { AlertCircle, ArrowDown, ArrowRight, ArrowUp, CheckCircle2 } from "lucide-react";
import { Badge } from "@/components/ui/shadcn/badge";
import { Button } from "@/components/ui/shadcn/button";
import { Dialog, DialogContent, DialogHeader, DialogTitle } from "@/components/ui/shadcn/dialog";
import { Spinner } from "@/components/ui/shadcn/spinner";
import useTraceWaterfall from "@/hooks/api/traces/useTraceWaterfall";
import type { Trace, WaterfallResponse } from "@/services/api/traces";
import { TraceSummaryStrip } from "../../trace/components/TraceSummaryStrip";
import { formatDuration, formatTimeAgo } from "../../utils";
import { deriveTraceRow } from "./traceRow";

interface CompareTracesDialogProps {
  traces: Trace[];
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onOpenTrace: (traceId: string) => void;
}

function ColumnHeader({
  trace,
  label,
  onOpenTrace
}: {
  trace: Trace;
  label: string;
  onOpenTrace: (traceId: string) => void;
}) {
  const row = deriveTraceRow(trace);
  return (
    <div className='flex flex-col gap-1.5'>
      <div className='flex items-center gap-2'>
        <Badge variant='secondary' className='text-xs'>
          {label}
        </Badge>
        {row.isError ? (
          <AlertCircle className='size-4 text-destructive' />
        ) : (
          <CheckCircle2 className='size-4 text-success' />
        )}
        <span className='truncate font-medium text-sm' title={row.title}>
          {row.title}
        </span>
      </div>
      <div className='flex items-center gap-2 text-muted-foreground text-xs'>
        <Badge variant='outline' className='text-xs'>
          {row.spanLabel}
        </Badge>
        {row.entityRef && <span className='truncate'>{row.entityRef}</span>}
        <span>{formatTimeAgo(row.timestamp)}</span>
        <Button
          variant='link'
          size='sm'
          className='h-auto p-0 text-xs'
          onClick={() => onOpenTrace(row.traceId)}
        >
          Open
        </Button>
      </div>
    </div>
  );
}

function SummaryPanel({
  trace,
  label,
  waterfall,
  isLoading,
  onOpenTrace
}: {
  trace: Trace;
  label: string;
  waterfall?: WaterfallResponse;
  isLoading: boolean;
  onOpenTrace: (traceId: string) => void;
}) {
  return (
    <div className='flex min-w-0 flex-col gap-3'>
      <ColumnHeader trace={trace} label={label} onOpenTrace={onOpenTrace} />
      {isLoading ? (
        <div className='flex h-32 items-center justify-center'>
          <Spinner className='size-6 text-muted-foreground' />
        </div>
      ) : waterfall ? (
        <TraceSummaryStrip
          summary={waterfall.summary}
          totalDurationMs={waterfall.totalDurationMs}
        />
      ) : (
        <p className='text-muted-foreground text-sm'>Could not load trace summary.</p>
      )}
    </div>
  );
}

interface DeltaRow {
  label: string;
  delta: number;
  formatted: string;
}

function buildDeltas(a: WaterfallResponse, b: WaterfallResponse): DeltaRow[] {
  const fmtNum = (n: number) => (n >= 0 ? "+" : "−") + Math.abs(n).toLocaleString();
  return [
    {
      label: "Duration",
      delta: b.totalDurationMs - a.totalDurationMs,
      formatted:
        (b.totalDurationMs - a.totalDurationMs >= 0 ? "+" : "−") +
        formatDuration(Math.abs(b.totalDurationMs - a.totalDurationMs))
    },
    {
      label: "Tokens",
      delta: b.summary.totalTokens - a.summary.totalTokens,
      formatted: fmtNum(b.summary.totalTokens - a.summary.totalTokens)
    },
    {
      label: "Spans",
      delta: b.summary.spanCount - a.summary.spanCount,
      formatted: fmtNum(b.summary.spanCount - a.summary.spanCount)
    },
    {
      label: "Errors",
      delta: b.summary.errorCount - a.summary.errorCount,
      formatted: fmtNum(b.summary.errorCount - a.summary.errorCount)
    }
  ];
}

function DeltaStrip({ a, b }: { a: WaterfallResponse; b: WaterfallResponse }) {
  return (
    <div className='rounded-lg border bg-muted/40 p-3'>
      <div className='mb-2 flex items-center gap-1 text-muted-foreground text-xs uppercase tracking-wide'>
        Difference <ArrowRight className='size-3' /> B − A
      </div>
      <div className='grid grid-cols-2 gap-2 sm:grid-cols-4'>
        {buildDeltas(a, b).map((d) => (
          <div key={d.label} className='flex flex-col gap-0.5'>
            <span className='text-[10px] text-muted-foreground uppercase tracking-wide'>
              {d.label}
            </span>
            <span className='flex items-center gap-1 font-semibold text-sm tabular-nums'>
              {d.delta !== 0 &&
                (d.delta > 0 ? (
                  <ArrowUp className='size-3 text-muted-foreground' />
                ) : (
                  <ArrowDown className='size-3 text-muted-foreground' />
                ))}
              {d.delta === 0 ? "—" : d.formatted}
            </span>
          </div>
        ))}
      </div>
    </div>
  );
}

/** Side-by-side comparison of two traces' summary metrics (Theme 3e). */
export function CompareTracesDialog({
  traces,
  open,
  onOpenChange,
  onOpenTrace
}: CompareTracesDialogProps) {
  const [a, b] = traces;
  const waterfallA = useTraceWaterfall(a?.traceId ?? "", open && !!a);
  const waterfallB = useTraceWaterfall(b?.traceId ?? "", open && !!b);

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className='max-w-4xl'>
        <DialogHeader>
          <DialogTitle>Compare traces</DialogTitle>
        </DialogHeader>
        {a && b && (
          <div className='flex flex-col gap-4'>
            <div className='grid grid-cols-1 gap-4 sm:grid-cols-2'>
              <SummaryPanel
                trace={a}
                label='A'
                waterfall={waterfallA.data}
                isLoading={waterfallA.isLoading}
                onOpenTrace={onOpenTrace}
              />
              <SummaryPanel
                trace={b}
                label='B'
                waterfall={waterfallB.data}
                isLoading={waterfallB.isLoading}
                onOpenTrace={onOpenTrace}
              />
            </div>
            {waterfallA.data && waterfallB.data && (
              <DeltaStrip a={waterfallA.data} b={waterfallB.data} />
            )}
          </div>
        )}
      </DialogContent>
    </Dialog>
  );
}
