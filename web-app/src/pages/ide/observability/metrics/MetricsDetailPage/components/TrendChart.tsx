import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/shadcn/card";
import { TrendingUp } from "lucide-react";
import { cn } from "@/libs/shadcn/utils";
import type { MetricDetailResponse } from "@/services/api/metrics";

interface TrendChartProps {
  detailData: MetricDetailResponse;
  daysFilter: number;
}

export default function TrendChart({
  detailData,
  daysFilter,
}: TrendChartProps) {
  const trends = detailData.usage_trend || [];

  if (!trends || trends.length === 0) {
    return (
      <Card className="lg:col-span-2">
        <CardHeader>
          <div className="flex items-center gap-2">
            <TrendingUp className="h-5 w-5 text-primary" />
            <CardTitle>Usage Trend</CardTitle>
          </div>
          <CardDescription>
            Query frequency over the last {daysFilter} days
          </CardDescription>
        </CardHeader>
        <CardContent>
          <div className="h-48 flex items-center justify-center text-muted-foreground">
            No trend data available
          </div>
        </CardContent>
      </Card>
    );
  }

  const maxUsage = Math.max(...trends.map((t) => t.count), 1);
  const avgUsage = Math.round(
    trends.reduce((sum, t) => sum + t.count, 0) / trends.length,
  );

  return (
    <Card className="lg:col-span-2">
      <CardHeader>
        <div className="flex items-center gap-2">
          <TrendingUp className="h-5 w-5 text-primary" />
          <CardTitle>Usage Trend</CardTitle>
        </div>
        <CardDescription>
          Query frequency over the last {daysFilter} days
        </CardDescription>
      </CardHeader>
      <CardContent>
        <div className="space-y-4">
          {/* Stats row */}
          <div className="flex items-center gap-6 text-sm">
            <div className="flex items-center gap-2">
              <div className="w-3 h-3 rounded-full bg-primary" />
              <span className="text-muted-foreground">Peak:</span>
              <span className="font-medium">{maxUsage}</span>
            </div>
            <div className="flex items-center gap-2">
              <div className="w-3 h-3 rounded-full bg-primary/50" />
              <span className="text-muted-foreground">Avg:</span>
              <span className="font-medium">{avgUsage}</span>
            </div>
          </div>

          {/* Chart */}
          <div className="relative h-48">
            {/* Horizontal grid lines */}
            <div className="absolute inset-0 flex flex-col justify-between pointer-events-none">
              {[...Array(5)].map((_, i) => (
                <div key={i} className="border-t border-muted/30" />
              ))}
            </div>

            {/* Bars */}
            <div className="absolute inset-0 flex items-end gap-px">
              {trends.map((point, index) => {
                const height = (point.count / maxUsage) * 100;
                const showLabel = index % Math.ceil(trends.length / 7) === 0;

                return (
                  <div
                    key={index}
                    className="flex-1 flex flex-col items-center h-full"
                  >
                    <div className="flex-1 w-full flex items-end justify-center px-0.5">
                      <div
                        className={cn(
                          "w-full rounded-t transition-all duration-300 cursor-pointer",
                          "bg-gradient-to-t from-primary to-primary/70",
                          "hover:from-primary hover:to-primary/90",
                          "relative group",
                        )}
                        style={{
                          height: `${height}%`,
                          minHeight: point.count > 0 ? "4px" : "0",
                        }}
                      >
                        {/* Tooltip */}
                        <div className="absolute bottom-full left-1/2 -translate-x-1/2 mb-2 hidden group-hover:block z-20">
                          <div className="bg-popover text-popover-foreground px-3 py-2 rounded-lg text-xs whitespace-nowrap border shadow-lg">
                            <p className="font-semibold">
                              {point.count} queries
                            </p>
                            <p className="text-muted-foreground">
                              {new Date(point.date).toLocaleDateString(
                                "en-US",
                                {
                                  weekday: "short",
                                  month: "short",
                                  day: "numeric",
                                },
                              )}
                            </p>
                          </div>
                        </div>
                      </div>
                    </div>
                    {showLabel && (
                      <div className="text-[10px] text-muted-foreground mt-2 whitespace-nowrap">
                        {new Date(point.date).toLocaleDateString("en-US", {
                          month: "short",
                          day: "numeric",
                        })}
                      </div>
                    )}
                  </div>
                );
              })}
            </div>
          </div>
        </div>
      </CardContent>
    </Card>
  );
}
