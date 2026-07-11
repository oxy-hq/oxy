import type { EChartsOption } from "echarts";
import { BarChart3 } from "lucide-react";
import { useMemo } from "react";
import EChart from "@/components/Echarts/EChart";
import { resolveColor, resolveColorWithAlpha } from "@/components/Echarts/resolveColor";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle
} from "@/components/ui/shadcn/card";
import { useExecutionHistogram } from "@/hooks/api/useExecutionAnalytics";
import type { HistogramResponse } from "../types";

const fmtMs = (ms: number) =>
  ms >= 1000 ? `${(ms / 1000).toFixed(ms >= 10000 ? 0 : 1)}s` : `${Math.round(ms)}ms`;

function buildOption(data: HistogramResponse): EChartsOption {
  // The server returns only populated log-buckets; fill the gaps so the x-axis
  // spacing stays faithful to the log scale (bucket index = log2(upperMs) − 1).
  const countByBucket = new Map<number, number>();
  for (const b of data.buckets) {
    countByBucket.set(Math.round(Math.log2(b.upperMs)) - 1, b.count);
  }
  const maxBucket = countByBucket.size ? Math.max(...countByBucket.keys()) : 0;
  const buckets = Array.from({ length: maxBucket + 1 }, (_, b) => ({
    upperMs: 2 ** (b + 1),
    count: countByBucket.get(b) ?? 0
  }));
  const labels = buckets.map((b) => fmtMs(b.upperMs));
  const counts = buckets.map((b) => b.count);

  // Map a percentile (ms) to the label of the first bucket that contains it.
  const markerLabel = (ms: number) => {
    const idx = buckets.findIndex((b) => b.upperMs >= ms);
    return idx >= 0 ? labels[idx] : labels[labels.length - 1];
  };
  const marker = (ms: number, name: string, token: string) => ({
    xAxis: markerLabel(ms),
    lineStyle: { color: resolveColor(token), type: "dashed" as const, width: 1.5 },
    label: {
      formatter: name,
      color: resolveColor(token),
      fontSize: 10,
      position: "insideEndTop" as const
    }
  });

  return {
    grid: { top: 16, bottom: 24, left: 40, right: 12 },
    tooltip: {
      trigger: "axis",
      formatter: (params: unknown) => {
        const p = (params as { name: string; value: number }[])[0];
        return `≤ ${p.name}<br/><b>${p.value.toLocaleString()}</b> executions`;
      }
    },
    xAxis: {
      type: "category",
      data: labels,
      axisLabel: { fontSize: 9, interval: 1 },
      axisTick: { show: false }
    },
    yAxis: { type: "value", splitLine: { show: true } },
    series: [
      {
        type: "bar",
        data: counts,
        itemStyle: { color: resolveColorWithAlpha("--vis-cyan", 0.85), borderRadius: [3, 3, 0, 0] },
        barCategoryGap: "18%",
        markLine: {
          symbol: "none",
          silent: true,
          data: [
            marker(data.p50Ms, "p50", "--pct-p50"),
            marker(data.p95Ms, "p95", "--pct-p95"),
            marker(data.p99Ms, "p99", "--pct-p99")
          ]
        }
      }
    ]
  };
}

interface LatencyHistogramProps {
  projectId: string;
  days: number;
}

export default function LatencyHistogram({ projectId, days }: LatencyHistogramProps) {
  const { data, isLoading } = useExecutionHistogram(projectId, days);
  const option = useMemo(() => (data ? buildOption(data) : null), [data]);

  return (
    <Card className='bg-transparent shadow-none'>
      <CardHeader className='pb-2'>
        <div className='flex items-center gap-2'>
          <BarChart3 className='h-5 w-5 text-primary' />
          <CardTitle>Latency Distribution</CardTitle>
        </div>
        <CardDescription>Execution wall time · log buckets · p50/p95/p99 marked</CardDescription>
      </CardHeader>
      <CardContent>
        {option ? (
          <EChart option={option} height={220} loading={isLoading} />
        ) : (
          <div className='flex h-[220px] items-center justify-center text-muted-foreground text-sm'>
            {isLoading ? "Loading…" : "No executions in range"}
          </div>
        )}
      </CardContent>
    </Card>
  );
}
