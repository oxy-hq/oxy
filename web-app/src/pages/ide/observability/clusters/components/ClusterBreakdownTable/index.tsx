import { useMemo } from "react";
import { Badge } from "@/components/ui/shadcn/badge";
import { cn } from "@/libs/shadcn/utils";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/shadcn/card";
import {
  Layers,
  Loader2,
  MessageSquare,
  Hash,
  Quote,
  CheckCircle,
  XCircle,
} from "lucide-react";
import type { ClusterSummary, ClusterMapPoint } from "@/services/api/traces";

interface ClusterBreakdownTableProps {
  clusters: ClusterSummary[];
  points: ClusterMapPoint[];
  isLoading: boolean;
  onClusterClick?: (clusterId: number) => void;
}

interface ClusterStats {
  clusterId: number;
  intentName: string;
  description: string;
  count: number;
  answeredCount: number;
  failedCount: number;
  successRate: number;
  color: string;
  sampleQuestions: string[];
}

function getSuccessRateBadgeColor(successRate: number): string {
  if (successRate >= 90) {
    return "bg-green-100 text-green-800 dark:bg-green-900/30 dark:text-green-400";
  }
  if (successRate >= 70) {
    return "bg-yellow-100 text-yellow-800 dark:bg-yellow-900/30 dark:text-yellow-400";
  }
  return "bg-red-100 text-red-800 dark:bg-red-900/30 dark:text-red-400";
}

function DistributionBar({
  success,
  error,
}: {
  success: number;
  error: number;
}) {
  return (
    <div className="flex h-2 w-full rounded-full overflow-hidden bg-muted">
      <div
        className="bg-emerald-500 transition-all"
        style={{ width: `${success}%` }}
        title={`Success: ${success.toFixed(1)}%`}
      />
      <div
        className="bg-rose-500 transition-all"
        style={{ width: `${error}%` }}
        title={`Error: ${error.toFixed(1)}%`}
      />
    </div>
  );
}

export default function ClusterBreakdownTable({
  clusters,
  points,
  isLoading,
}: ClusterBreakdownTableProps) {
  const clusterStats = useMemo(() => {
    // Group points by cluster
    const pointsByCluster = new Map<number, ClusterMapPoint[]>();
    points.forEach((point) => {
      const existing = pointsByCluster.get(point.clusterId) || [];
      existing.push(point);
      pointsByCluster.set(point.clusterId, existing);
    });

    // Calculate stats for each cluster
    const stats: ClusterStats[] = clusters
      .filter((c) => c.clusterId !== -1)
      .map((cluster) => {
        const clusterPoints = pointsByCluster.get(cluster.clusterId) || [];
        let answeredCount = 0;
        let failedCount = 0;

        clusterPoints.forEach((point) => {
          // Use status from API if available
          if (point.status === "ok") {
            answeredCount++;
          } else if (point.status === "error") {
            failedCount++;
          }
        });

        const total = clusterPoints.length;
        const successRate = total > 0 ? (answeredCount / total) * 100 : 0;

        return {
          clusterId: cluster.clusterId,
          intentName: cluster.intentName,
          description: cluster.description,
          count: cluster.count,
          answeredCount,
          failedCount,
          successRate,
          color: cluster.color,
          sampleQuestions: cluster.sampleQuestions,
        };
      })
      .sort((a, b) => b.count - a.count);

    return stats;
  }, [clusters, points]);

  if (isLoading) {
    return (
      <Card>
        <CardHeader>
          <div className="flex items-center gap-2">
            <Layers className="h-5 w-5 text-primary" />
            <CardTitle>Cluster Breakdown</CardTitle>
            <Loader2 className="h-4 w-4 animate-spin text-muted-foreground" />
          </div>
          <CardDescription>Performance metrics by cluster</CardDescription>
        </CardHeader>
        <CardContent>
          <div className="animate-pulse space-y-2">
            {[1, 2, 3].map((i) => (
              <div key={i} className="h-10 bg-muted rounded" />
            ))}
          </div>
        </CardContent>
      </Card>
    );
  }

  if (clusterStats.length === 0) {
    return (
      <Card>
        <CardHeader>
          <div className="flex items-center gap-2">
            <Layers className="h-5 w-5 text-primary" />
            <CardTitle>Cluster Breakdown</CardTitle>
          </div>
          <CardDescription>Performance metrics by cluster</CardDescription>
        </CardHeader>
        <CardContent>
          <p className="text-sm text-muted-foreground text-center py-4">
            No cluster data available
          </p>
        </CardContent>
      </Card>
    );
  }

  return (
    <Card>
      <CardHeader>
        <div className="flex items-center gap-2">
          <Layers className="h-5 w-5 text-primary" />
          <CardTitle>Cluster Breakdown</CardTitle>
        </div>
        <CardDescription>Performance metrics by cluster</CardDescription>
      </CardHeader>
      <CardContent>
        <div className="space-y-4">
          {clusterStats.map((cluster, index) => (
            <div
              key={cluster.clusterId}
              className="relative border rounded-xl p-5 bg-card hover:bg-muted/30 transition-colors"
            >
              {/* Top Row: Color badge, Intent Name, Tags */}
              <div className="flex items-start justify-between gap-4 mb-3">
                <div className="flex items-center gap-3 min-w-0 flex-1">
                  <div
                    className="w-3 h-3 rounded-full shrink-0"
                    style={{ backgroundColor: cluster.color }}
                  />
                  <div className="flex items-center gap-2 flex-wrap">
                    <h4 className="font-semibold text-base">
                      {cluster.intentName}
                    </h4>
                    <Badge variant="outline" className="text-xs font-normal">
                      <Hash className="h-3 w-3 mr-1" />
                      {cluster.clusterId}
                    </Badge>
                    <Badge variant="outline" className="text-xs font-normal">
                      <MessageSquare className="h-3 w-3 mr-1" />
                      {cluster.count} questions
                    </Badge>
                  </div>
                </div>
                {/* Success Rate Badge - prominent on the right */}
                <Badge
                  className={cn(
                    "font-semibold text-sm px-3 py-1 shrink-0",
                    getSuccessRateBadgeColor(cluster.successRate),
                  )}
                >
                  {cluster.successRate.toFixed(1)}% success
                </Badge>
              </div>

              {/* Description - Full display */}
              {cluster.description && (
                <p className="text-sm text-muted-foreground mb-4 leading-relaxed">
                  {cluster.description}
                </p>
              )}

              {/* Success/Error Stats */}
              <div className="flex items-center gap-6 text-sm mb-4">
                <div className="flex items-center gap-1.5">
                  <CheckCircle className="h-4 w-4 text-emerald-500" />
                  <span className="font-medium text-emerald-600 dark:text-emerald-400">
                    {cluster.answeredCount}
                  </span>
                  <span className="text-muted-foreground">success</span>
                </div>
                <div className="flex items-center gap-1.5">
                  <XCircle className="h-4 w-4 text-rose-500" />
                  <span className="font-medium text-rose-600 dark:text-rose-400">
                    {cluster.failedCount}
                  </span>
                  <span className="text-muted-foreground">error</span>
                </div>
                <div className="flex-1 flex items-center gap-3">
                  <DistributionBar
                    success={
                      cluster.count > 0
                        ? (cluster.answeredCount / cluster.count) * 100
                        : 0
                    }
                    error={
                      cluster.count > 0
                        ? (cluster.failedCount / cluster.count) * 100
                        : 0
                    }
                  />
                  <span className="text-xs text-muted-foreground shrink-0">
                    {cluster.count > 0
                      ? ((cluster.answeredCount / cluster.count) * 100).toFixed(
                          0,
                        )
                      : 0}
                    % /{" "}
                    {cluster.count > 0
                      ? ((cluster.failedCount / cluster.count) * 100).toFixed(0)
                      : 0}
                    %
                  </span>
                </div>
              </div>

              {/* Sample Questions */}
              {cluster.sampleQuestions.length > 0 && (
                <div className="pt-4 border-t mt-4">
                  {(() => {
                    const uniqueQuestions = [
                      ...new Set(cluster.sampleQuestions),
                    ];
                    return (
                      <>
                        <div className="flex items-center gap-2 text-xs text-muted-foreground mb-2">
                          <Quote className="h-3.5 w-3.5" />
                          <span>
                            Sample Questions ({uniqueQuestions.length})
                          </span>
                        </div>
                        <div className="flex flex-col gap-2">
                          {uniqueQuestions.map((q, idx) => (
                            <Badge
                              key={idx}
                              variant="secondary"
                              className="font-normal text-xs py-1.5 px-3 w-fit max-w-full"
                            >
                              <span className="truncate">{q}</span>
                            </Badge>
                          ))}
                        </div>
                      </>
                    );
                  })()}
                </div>
              )}

              {/* Rank indicator */}
              {index < 3 && (
                <div className="absolute -top-2 -right-2">
                  <Badge
                    className={cn(
                      "text-xs font-bold",
                      index === 0 && "bg-yellow-500 text-yellow-950",
                      index === 1 && "bg-slate-400 text-slate-950",
                      index === 2 && "bg-amber-600 text-amber-950",
                    )}
                  >
                    #{index + 1}
                  </Badge>
                </div>
              )}
            </div>
          ))}
        </div>
      </CardContent>
    </Card>
  );
}
