import { useMemo } from "react";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/shadcn/card";
import { Network, Lightbulb, Loader2 } from "lucide-react";
import type { ClusterSummary, ClusterMapPoint } from "@/services/api/traces";

interface SidebarProps {
  clusters: ClusterSummary[];
  points: ClusterMapPoint[];
  isLoading: boolean;
}

export default function Sidebar({ clusters, points, isLoading }: SidebarProps) {
  const topClusters = useMemo(() => {
    const validClusters = clusters.filter((c) => c.clusterId !== -1);
    const totalCount = validClusters.reduce((sum, c) => sum + c.count, 0);

    return validClusters
      .sort((a, b) => b.count - a.count)
      .slice(0, 5)
      .map((c) => ({
        intentName: c.intentName,
        count: c.count,
        percentage: totalCount > 0 ? (c.count / totalCount) * 100 : 0,
        color: c.color,
      }));
  }, [clusters]);

  const insights = useMemo(() => {
    const outlierCluster = clusters.find((c) => c.clusterId === -1);
    const outlierCount = outlierCluster?.count ?? 0;
    const totalPoints = points.length;

    const results: string[] = [];

    if (topClusters.length > 0) {
      const topCluster = topClusters[0];
      results.push(
        `"${topCluster.intentName}" is your most common query pattern, representing ${topCluster.percentage.toFixed(0)}% of all queries.`,
      );
    }

    if (outlierCount > 0 && totalPoints > 0) {
      const outlierPercentage = (outlierCount / totalPoints) * 100;
      if (outlierPercentage > 30) {
        results.push(
          `High outlier rate (${outlierPercentage.toFixed(0)}%) suggests many unique or uncommon queries.`,
        );
      } else if (outlierPercentage < 10) {
        results.push(
          `Low outlier rate (${outlierPercentage.toFixed(0)}%) indicates good semantic coverage.`,
        );
      }
    }

    return results;
  }, [clusters, points, topClusters]);

  return (
    <Card className="h-full">
      <CardHeader className="pb-2">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            <Network className="h-5 w-5 text-primary" />
            <CardTitle>Top Clusters</CardTitle>
            {isLoading && (
              <Loader2 className="h-4 w-4 animate-spin text-muted-foreground" />
            )}
          </div>
        </div>
        <CardDescription>Most frequent query patterns</CardDescription>
      </CardHeader>
      <CardContent className="pt-0">
        <div className="space-y-3">
          {topClusters.map((cluster, index) => (
            <div key={cluster.intentName} className="flex items-center gap-3">
              <div
                className="w-6 h-6 rounded-md flex items-center justify-center text-xs font-semibold shrink-0"
                style={{
                  backgroundColor: `${cluster.color}20`,
                  color: cluster.color,
                }}
              >
                {index + 1}
              </div>
              <div className="flex-1 min-w-0">
                <div className="flex items-center justify-between mb-1">
                  <span className="font-medium truncate text-sm">
                    {cluster.intentName}
                  </span>
                  <span className="text-muted-foreground shrink-0 ml-2 text-sm">
                    {cluster.count}
                  </span>
                </div>
                <div className="h-1.5 bg-muted rounded-full overflow-hidden">
                  <div
                    className="h-full rounded-full transition-all"
                    style={{
                      width: `${cluster.percentage}%`,
                      backgroundColor: cluster.color,
                    }}
                  />
                </div>
              </div>
              <span className="text-xs text-muted-foreground w-10 text-right shrink-0">
                {cluster.percentage.toFixed(0)}%
              </span>
            </div>
          ))}
          {topClusters.length === 0 && !isLoading && (
            <p className="text-sm text-muted-foreground text-center py-2">
              No clusters found
            </p>
          )}
        </div>

        {/* Compact Insights */}
        {insights.length > 0 && (
          <div className="mt-4 pt-4 border-t">
            <div className="flex items-center gap-1.5 mb-2">
              <Lightbulb className="h-3.5 w-3.5 text-amber-500" />
              <span className="text-xs font-medium">Insights</span>
            </div>
            <div className="space-y-1.5">
              {insights.map((insight, index) => (
                <p key={index} className="text-xs text-muted-foreground">
                  {insight}
                </p>
              ))}
            </div>
          </div>
        )}
      </CardContent>
    </Card>
  );
}
