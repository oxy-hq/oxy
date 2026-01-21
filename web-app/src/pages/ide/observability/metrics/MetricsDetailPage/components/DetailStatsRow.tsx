import { Card, CardContent } from "@/components/ui/shadcn/card";
import {
  BarChart3,
  TrendingUp,
  TrendingDown,
  Minus,
  LucideBot,
  LucideWorkflow,
} from "lucide-react";
import { cn } from "@/libs/shadcn/utils";
import type { MetricDetailResponse } from "@/services/api/metrics";

function TrendIcon({
  isPositive,
  isNegative,
}: {
  isPositive?: boolean;
  isNegative?: boolean;
}) {
  if (isPositive) {
    return (
      <div className="p-2 rounded-lg bg-green-500/10">
        <TrendingUp className="h-5 w-5 text-green-400" />
      </div>
    );
  }
  if (isNegative) {
    return (
      <div className="p-2 rounded-lg bg-red-500/10">
        <TrendingDown className="h-5 w-5 text-red-400" />
      </div>
    );
  }
  return (
    <div className="p-2 rounded-lg bg-muted">
      <Minus className="h-5 w-5 text-muted-foreground" />
    </div>
  );
}

interface DetailStatsRowProps {
  detailData: MetricDetailResponse;
}

export default function DetailStatsRow({ detailData }: DetailStatsRowProps) {
  const totalQueries = detailData.total_queries;
  const trendVsLastPeriod = detailData.trend_vs_last_period;
  const viaAgent = detailData.via_agent;
  const viaWorkflow = detailData.via_workflow;

  const trend = trendVsLastPeriod;
  const isPositive = trend?.startsWith("+");
  const isNegative = trend?.startsWith("-");

  return (
    <div className="grid grid-cols-1 md:grid-cols-4 gap-4">
      <Card className="overflow-hidden">
        <CardContent className="p-4">
          <div className="flex items-center gap-3">
            <div className="p-2 rounded-lg bg-primary/10">
              <BarChart3 className="h-5 w-5 text-primary" />
            </div>
            <div>
              <p className="text-2xl font-bold">
                {totalQueries.toLocaleString()}
              </p>
              <p className="text-xs text-muted-foreground">Total Queries</p>
            </div>
          </div>
        </CardContent>
      </Card>

      <Card className="overflow-hidden">
        <CardContent className="p-4">
          <div className="flex items-center gap-3">
            <TrendIcon isPositive={isPositive} isNegative={isNegative} />
            <div>
              <p
                className={cn(
                  "text-2xl font-bold",
                  isPositive && "text-green-400",
                  isNegative && "text-red-400",
                )}
              >
                {trend || "—"}
              </p>
              <p className="text-xs text-muted-foreground">vs Last Period</p>
            </div>
          </div>
        </CardContent>
      </Card>

      <Card className="overflow-hidden">
        <CardContent className="p-4">
          <div className="flex items-center gap-3">
            <div className="p-2 rounded-lg bg-blue-500/10">
              <LucideBot className="h-5 w-5 text-blue-400" />
            </div>
            <div>
              <p className="text-2xl font-bold">{viaAgent}</p>
              <p className="text-xs text-muted-foreground">Via Agent</p>
            </div>
          </div>
        </CardContent>
      </Card>

      <Card className="overflow-hidden">
        <CardContent className="p-4">
          <div className="flex items-center gap-3">
            <div className="p-2 rounded-lg bg-purple-500/10">
              <LucideWorkflow className="h-5 w-5 text-purple-400" />
            </div>
            <div>
              <p className="text-2xl font-bold">{viaWorkflow}</p>
              <p className="text-xs text-muted-foreground">Via Workflow</p>
            </div>
          </div>
        </CardContent>
      </Card>
    </div>
  );
}
