import type { EChartsOption } from "echarts";
import { TrendingUp } from "lucide-react";
import { useMemo } from "react";
import EChart from "@/components/Echarts/EChart";
import { resolveColor } from "@/components/Echarts/resolveColor";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle
} from "@/components/ui/shadcn/card";
import { useExecutionPercentiles } from "@/hooks/api/useExecutionAnalytics";
import type { PercentilesResponse } from "../types";

const fmtMs = (ms: number) => (ms >= 1000 ? `${(ms / 1000).toFixed(1)}s` : `${Math.round(ms)}ms`);

function buildOption(data: PercentilesResponse): EChartsOption {
  const dates = data.series.map((p) => p.date);
  const line = (name: string, token: string, values: number[]) => ({
    name,
    type: "line" as const,
    data: values,
    showSymbol: false,
    smooth: true,
    lineStyle: { width: 2, color: resolveColor(token) },
    itemStyle: { color: resolveColor(token) }
  });

  return {
    grid: { top: 16, bottom: 24, left: 44, right: 16 },
    legend: {
      data: ["p50", "p95", "p99"],
      right: 8,
      top: 0,
      itemWidth: 14,
      textStyle: { fontSize: 11 }
    },
    tooltip: { trigger: "axis", valueFormatter: (v: unknown) => fmtMs(Number(v)) },
    xAxis: { type: "category", data: dates, axisLabel: { fontSize: 9 }, axisTick: { show: false } },
    yAxis: { type: "value", axisLabel: { formatter: (v: number) => fmtMs(v), fontSize: 9 } },
    series: [
      line(
        "p50",
        "--pct-p50",
        data.series.map((p) => p.p50Ms)
      ),
      line(
        "p95",
        "--pct-p95",
        data.series.map((p) => p.p95Ms)
      ),
      line(
        "p99",
        "--pct-p99",
        data.series.map((p) => p.p99Ms)
      )
    ]
  };
}

interface PercentileTrendProps {
  projectId: string;
  days: number;
}

export default function PercentileTrend({ projectId, days }: PercentileTrendProps) {
  const { data, isLoading } = useExecutionPercentiles(projectId, days);
  const option = useMemo(() => (data ? buildOption(data) : null), [data]);

  return (
    <Card className='bg-transparent shadow-none'>
      <CardHeader className='pb-2'>
        <div className='flex items-center gap-2'>
          <TrendingUp className='h-5 w-5 text-primary' />
          <CardTitle>Latency Percentiles</CardTitle>
        </div>
        <CardDescription>p50 / p95 / p99 over time</CardDescription>
      </CardHeader>
      <CardContent>
        {option && data && data.series.length > 0 ? (
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
