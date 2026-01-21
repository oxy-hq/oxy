import { useMemo } from "react";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/shadcn/card";
import { Calendar } from "lucide-react";
import SourceTypeBadge from "./SourceTypeBadge";
import UsageContextCard from "./UsageContextCard";
import type { MetricDetailResponse } from "@/services/api/metrics";

interface RecentUsageSectionProps {
  detailData: MetricDetailResponse;
  metricName: string;
  onTraceClick: (traceId: string) => void;
}

export default function RecentUsageSection({
  detailData,
  metricName,
  onTraceClick,
}: RecentUsageSectionProps) {
  const recentUsage = detailData.recent_usage;

  // Compute source type breakdown
  const sourceBreakdown = useMemo(() => {
    const breakdown: Record<string, number> = {};
    if (recentUsage) {
      recentUsage.forEach((usage) => {
        const sourceType = usage.source_type || "agent";
        breakdown[sourceType] = (breakdown[sourceType] || 0) + 1;
      });
    }
    return breakdown;
  }, [recentUsage]);
  return (
    <Card>
      <CardHeader>
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            <Calendar className="h-5 w-5 text-primary" />
            <CardTitle>Recent Usage</CardTitle>
          </div>
          <div className="flex items-center gap-2">
            {Object.entries(sourceBreakdown).map(([type, count]) => (
              <div key={type} className="flex items-center gap-1">
                <SourceTypeBadge sourceType={type} />
                <span className="text-xs text-muted-foreground ml-1">
                  {count}
                </span>
              </div>
            ))}
          </div>
        </div>
        <CardDescription>
          Last {recentUsage.length} queries referencing this metric
        </CardDescription>
      </CardHeader>
      <CardContent>
        {recentUsage.length > 0 ? (
          <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
            {recentUsage.map((usage, index) => (
              <UsageContextCard
                key={index}
                usage={usage}
                metricName={metricName}
                onTraceClick={onTraceClick}
              />
            ))}
          </div>
        ) : (
          <p className="text-sm text-muted-foreground">
            No recent usage data available
          </p>
        )}
      </CardContent>
    </Card>
  );
}
