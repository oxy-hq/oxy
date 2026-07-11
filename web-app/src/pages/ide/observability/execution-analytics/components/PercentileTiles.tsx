import type { EChartsOption } from "echarts";
import { useMemo } from "react";
import EChart from "@/components/Echarts/EChart";
import { resolveColor, resolveColorWithAlpha } from "@/components/Echarts/resolveColor";
import { Card } from "@/components/ui/shadcn/card";
import { useExecutionPercentiles } from "@/hooks/api/useExecutionAnalytics";
import type { ExecutionSummary, PercentileTimePoint } from "../types";

const fmtMs = (ms: number) => (ms >= 1000 ? `${(ms / 1000).toFixed(2)}s` : `${Math.round(ms)}ms`);

function sparkOption(values: number[], token: string): EChartsOption {
  const color = resolveColor(token);
  return {
    grid: { top: 4, bottom: 4, left: 2, right: 2 },
    xAxis: { type: "category", show: false, data: values.map((_, i) => i) },
    yAxis: { type: "value", show: false, scale: true },
    tooltip: { show: false },
    series: [
      {
        type: "line",
        data: values,
        showSymbol: false,
        smooth: true,
        lineStyle: { width: 2, color },
        areaStyle: { color: resolveColorWithAlpha(token, 0.12) }
      }
    ]
  };
}

interface TileProps {
  label: string;
  value: string;
  token: string;
  series?: number[];
  hot?: boolean;
}

function Tile({ label, value, token, series, hot }: TileProps) {
  const option = useMemo(
    () => (series && series.length > 1 ? sparkOption(series, token) : null),
    [series, token]
  );
  return (
    <Card className='relative flex flex-col gap-1 overflow-hidden p-3'>
      <span className='text-[0.65rem] text-muted-foreground uppercase tracking-wide'>{label}</span>
      <span
        className='font-semibold text-lg tabular-nums'
        style={{ color: hot ? resolveColor("--error") : undefined }}
      >
        {value}
      </span>
      {option ? <EChart option={option} height={26} className='mt-0.5' /> : null}
    </Card>
  );
}

interface PercentileTilesProps {
  projectId: string;
  days: number;
  summary: ExecutionSummary;
}

/** Blended error rate (%) derived from the summary's per-category success rates. */
function errorRateFromSummary(s: ExecutionSummary): number {
  const total = s.totalExecutions;
  if (total === 0) return 0;
  const successes =
    (s.verifiedCount * s.successRateVerified + s.generatedCount * s.successRateGenerated) / 100;
  return Math.max(0, 100 - (successes / total) * 100);
}

export default function PercentileTiles({ projectId, days, summary }: PercentileTilesProps) {
  const { data } = useExecutionPercentiles(projectId, days);
  const series = data?.series ?? [];
  const pick = (key: keyof PercentileTimePoint) => series.map((p) => Number(p[key] ?? 0));

  const overall = data?.overall;
  const errorRate = errorRateFromSummary(summary);

  return (
    <div className='grid grid-cols-2 gap-3 md:grid-cols-4'>
      <Tile
        label='p50 latency'
        value={overall ? fmtMs(overall.p50Ms) : "—"}
        token='--pct-p50'
        series={pick("p50Ms")}
      />
      <Tile
        label='p95 latency'
        value={overall ? fmtMs(overall.p95Ms) : "—"}
        token='--pct-p95'
        series={pick("p95Ms")}
      />
      <Tile
        label='p99 latency'
        value={overall ? fmtMs(overall.p99Ms) : "—"}
        token='--pct-p99'
        series={pick("p99Ms")}
      />
      <Tile
        label='Error rate'
        value={`${errorRate.toFixed(1)}%`}
        token='--error'
        hot={errorRate > 0}
      />
    </div>
  );
}
